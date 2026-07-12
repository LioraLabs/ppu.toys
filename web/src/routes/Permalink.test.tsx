// @vitest-environment jsdom
import { describe, it, expect, afterEach, vi } from "vitest";
import "@testing-library/jest-dom/vitest";
import { render, screen, cleanup, fireEvent, waitFor } from "@testing-library/react";
import { MemoryRouter, Routes, Route } from "react-router-dom";
import type { ToyFull } from "../api/apiClient";

const navigate = vi.fn();
vi.mock("react-router-dom", async (orig) => ({
  ...(await orig<typeof import("react-router-dom")>()),
  useNavigate: () => navigate,
}));
vi.mock("../api/apiClient", () => ({ getToy: vi.fn(), forkToy: vi.fn() }));
vi.mock("../api/session", () => ({ useSession: () => ({ user: { id: "1", handle: "ada" }, loading: false }) }));
// Player wiring is covered by its own test; stub it here.
vi.mock("../components/ReadOnlyPlayer", () => ({ ReadOnlyPlayer: () => <div>player</div> }));
import { getToy, forkToy } from "../api/apiClient";

const toy: ToyFull = {
  id: "abc", title: "Dusk", description: "a toy", state: "published",
  files: [{ name: "main.lua", source: "-- code here" }],
  sources: [], heartCount: 2, hearted: false, forkedFrom: null,
  author: { id: "9", handle: "ada", avatar: null },
};
const mockGetToy = getToy as ReturnType<typeof vi.fn>;
const mockFork = forkToy as ReturnType<typeof vi.fn>;
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
