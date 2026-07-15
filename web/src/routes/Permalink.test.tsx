// @vitest-environment jsdom
import { describe, it, expect, afterEach, vi } from "vitest";
import "@testing-library/jest-dom/vitest";
import { render, screen, cleanup, fireEvent, waitFor } from "@testing-library/react";
import { MemoryRouter, Routes, Route } from "react-router-dom";
import { makeToyFull } from "../fixtures";

const navigate = vi.fn();
vi.mock("react-router-dom", async (orig) => ({
  ...(await orig<typeof import("react-router-dom")>()),
  useNavigate: () => navigate,
}));
vi.mock("../api/apiClient", () => ({ getToy: vi.fn(), forkToy: vi.fn() }));
vi.mock("../api/session", () => ({ useSession: () => ({ user: { id: "1", handle: "ada" }, loading: false }) }));
// Player wiring is covered by its own test; stub it here.
vi.mock("../components/ReadOnlyPlayer", () => ({ ReadOnlyPlayer: () => <div>player</div> }));
// openCloudToy touches IndexedDB (via createSketch/openSketchStore); it has
// its own coverage in studio/cloud/openCloudToy.test.ts, so stub it here.
vi.mock("../studio/cloud/openCloudToy", () => ({ openCloudToy: vi.fn() }));
import { getToy, forkToy } from "../api/apiClient";
import { openCloudToy } from "../studio/cloud/openCloudToy";

const toy = makeToyFull({
  id: "abc", description: "a toy", heartCount: 2,
  files: [{ name: "main.lua", source: "-- code here" }],
  author: { id: "9", handle: "ada", avatar: null },
});
const fork1 = makeToyFull({
  id: "fork1", title: "Dusk (fork)", description: "a toy", state: "draft",
  files: [{ name: "main.lua", source: "-- code here" }],
  heartCount: 0, forkedFrom: "abc",
});
const mockGetToy = getToy as ReturnType<typeof vi.fn>;
const mockFork = forkToy as ReturnType<typeof vi.fn>;
const mockOpenCloudToy = openCloudToy as ReturnType<typeof vi.fn>;
afterEach(() => { cleanup(); vi.clearAllMocks(); });

import { Permalink } from "./Permalink";
function renderAt(id = "abc") {
  return render(
    <MemoryRouter initialEntries={[`/t/${id}`]}>
      <Routes><Route path="/t/:id" element={<Permalink />} /></Routes>
    </MemoryRouter>,
  );
}

describe("Permalink", () => {
  it("fetches the toy and shows title, author, code and the player", async () => {
    mockGetToy.mockResolvedValue(toy);
    renderAt();
    expect(await screen.findByText("Dusk")).toBeInTheDocument();
    expect(screen.getByText("player")).toBeInTheDocument();
    expect(screen.getByText(/-- code here/)).toBeInTheDocument();
    expect(mockGetToy).toHaveBeenCalledWith("abc");
  });

  it("forks and navigates to /studio", async () => {
    mockGetToy.mockResolvedValue(toy);
    mockFork.mockResolvedValue({ id: "fork1" });
    renderAt();
    await screen.findByText("Dusk");
    fireEvent.click(screen.getByRole("button", { name: /fork/i }));
    await waitFor(() => expect(mockFork).toHaveBeenCalledWith("abc"));
    await waitFor(() => expect(navigate).toHaveBeenCalledWith("/studio"));
  });

  it("loads the fork and opens it in the studio before navigating", async () => {
    mockGetToy.mockImplementation((id: string) => Promise.resolve(id === "fork1" ? fork1 : toy));
    mockFork.mockResolvedValue({ id: "fork1" });
    renderAt();
    await screen.findByText("Dusk");
    fireEvent.click(screen.getByRole("button", { name: /fork/i }));

    await waitFor(() => expect(navigate).toHaveBeenCalledWith("/studio"));

    expect(mockFork).toHaveBeenCalledWith("abc");
    expect(mockGetToy).toHaveBeenCalledWith("fork1");
    expect(mockOpenCloudToy).toHaveBeenCalledWith(fork1);

    // Ordering matters: clone → load → open → navigate.
    const forkOrder = mockFork.mock.invocationCallOrder[0];
    const getForkOrder = mockGetToy.mock.invocationCallOrder.find((_, i) => mockGetToy.mock.calls[i][0] === "fork1")!;
    const openOrder = mockOpenCloudToy.mock.invocationCallOrder[0];
    expect(forkOrder).toBeLessThan(getForkOrder);
    expect(getForkOrder).toBeLessThan(openOrder);
  });

  it("shows a not-found message when the toy 404s", async () => {
    mockGetToy.mockRejectedValue(new Error("GET /api/toys/nope → 404"));
    renderAt("nope");
    expect(await screen.findByText(/not found/i)).toBeInTheDocument();
  });

  it("surfaces an error and stays on the page when fork fails", async () => {
    mockGetToy.mockResolvedValue(toy);
    mockFork.mockRejectedValue(new Error("POST /api/toys/abc/fork → 500"));
    renderAt();
    await screen.findByText("Dusk");
    fireEvent.click(screen.getByRole("button", { name: /fork/i }));
    expect(await screen.findByRole("alert")).toHaveTextContent(/fork failed/i);
    expect(navigate).not.toHaveBeenCalled();
  });
});
