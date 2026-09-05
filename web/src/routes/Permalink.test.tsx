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
vi.mock("../api/apiClient", () => ({ getToy: vi.fn() }));
const session = vi.hoisted(() => ({
  user: { id: "1", handle: "ada" } as { id: string; handle: string } | null,
}));
vi.mock("../api/session", () => ({
  useSession: () => ({ user: session.user, loading: false }),
}));
// Player wiring is covered by its own test; stub it here.
vi.mock("../components/ReadOnlyPlayer", () => ({ ReadOnlyPlayer: () => <div>player</div> }));
// openCloudToy touches IndexedDB (via createSketch/openSketchStore); it has
// its own coverage in studio/cloud/openCloudToy.test.ts, so stub it here.
vi.mock("../studio/cloud/openCloudToy", () => ({ openCloudToy: vi.fn() }));
import { getToy } from "../api/apiClient";
import { openCloudToy } from "../studio/cloud/openCloudToy";

const toy = makeToyFull({
  id: "abc",
  description: "a toy",
  heartCount: 2,
  files: [{ name: "main.lua", source: "-- code here" }],
  author: { id: "9", handle: "ada", avatar: null },
});
const mockGetToy = getToy as ReturnType<typeof vi.fn>;
const mockOpenCloudToy = openCloudToy as ReturnType<typeof vi.fn>;
afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  session.user = { id: "1", handle: "ada" };
});

import { Permalink } from "./Permalink";
function renderAt(id = "abc") {
  return render(
    <MemoryRouter initialEntries={[`/t/${id}`]}>
      <Routes>
        <Route path="/t/:id" element={<Permalink />} />
      </Routes>
    </MemoryRouter>,
  );
}

function renderRaw(id = "abc") {
  return render(
    <MemoryRouter initialEntries={[`/t/${id}/raw`]}>
      <Routes>
        <Route path="/t/:id/raw" element={<Permalink raw />} />
      </Routes>
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
    expect(screen.getByRole("link", { name: "Raw output" })).toHaveAttribute("href", "/t/abc/raw");
  });

  it("offers a dedicated player", async () => {
    mockGetToy.mockResolvedValue(toy);
    renderAt();
    expect(await screen.findByRole("link", { name: "Play full screen" })).toHaveAttribute(
      "href",
      "/t/abc/play",
    );
  });

  it("renders only the player on the raw output route", async () => {
    mockGetToy.mockResolvedValue(toy);
    const { container } = renderRaw();
    await screen.findByText("player");
    expect(container.querySelector(".raw-output")).toBeInTheDocument();
    expect(screen.queryByText("Dusk")).not.toBeInTheDocument();
    expect(screen.queryByText(/-- code here/)).not.toBeInTheDocument();
  });

  it("switches source tabs with the keyboard", async () => {
    mockGetToy.mockResolvedValue(
      makeToyFull({
        files: [
          { name: "main.lua", source: "-- main" },
          { name: "fx.lua", source: "-- effects" },
        ],
      }),
    );
    renderAt();
    const main = await screen.findByRole("tab", { name: "main.lua" });
    fireEvent.keyDown(main, { key: "ArrowRight" });
    expect(screen.getByRole("tabpanel")).toHaveTextContent("-- effects");
    expect(screen.getByRole("tab", { name: "fx.lua" })).toHaveFocus();
  });

  it("fork opens the ORIGINAL toy in the studio with no server write, then navigates", async () => {
    mockGetToy.mockResolvedValue(toy);
    renderAt();
    await screen.findByText("Dusk");
    fireEvent.click(screen.getByRole("button", { name: /fork/i }));

    await waitFor(() => expect(navigate).toHaveBeenCalledWith("/studio"));

    // Fork is lineage recorded at publish time: the studio opens the same toy
    // (origin = someone else's) and the publish dialog does the rest.
    expect(mockOpenCloudToy).toHaveBeenCalledTimes(1);
    expect(mockOpenCloudToy).toHaveBeenCalledWith(toy);
    // The only fetch is the page's own load — no fork id, no second getToy.
    expect(mockGetToy).toHaveBeenCalledTimes(1);
    expect(mockGetToy).toHaveBeenCalledWith("abc");
    expect(mockOpenCloudToy.mock.invocationCallOrder[0]).toBeLessThan(
      navigate.mock.invocationCallOrder[0],
    );
  });

  it("fork works signed out — the Studio needs no account until publish", async () => {
    session.user = null;
    mockGetToy.mockResolvedValue(toy);
    renderAt();
    await screen.findByText("Dusk");
    fireEvent.click(screen.getByRole("button", { name: /fork/i }));

    await waitFor(() => expect(navigate).toHaveBeenCalledWith("/studio"));
    expect(mockOpenCloudToy).toHaveBeenCalledWith(toy);
  });

  it("shows a retryable error when the toy cannot load", async () => {
    mockGetToy.mockRejectedValue(new Error("GET /api/toys/nope → 404"));
    renderAt("nope");
    expect(await screen.findByRole("alert")).toHaveTextContent(/couldn’t load/i);
    fireEvent.click(screen.getByRole("button", { name: /try again/i }));
    await waitFor(() => expect(mockGetToy).toHaveBeenCalledTimes(2));
  });

  it("surfaces an error and stays on the page when opening the fork fails", async () => {
    mockGetToy.mockResolvedValue(toy);
    mockOpenCloudToy.mockRejectedValue(new Error("IndexedDB unavailable"));
    renderAt();
    await screen.findByText("Dusk");
    fireEvent.click(screen.getByRole("button", { name: /fork/i }));
    expect(await screen.findByRole("alert")).toHaveTextContent(/fork failed/i);
    expect(navigate).not.toHaveBeenCalled();
  });
});
