// @vitest-environment jsdom
import { describe, it, expect, afterEach, vi } from "vitest";
import "@testing-library/jest-dom/vitest";
import { render, screen, cleanup, fireEvent, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { Wall } from "./Wall";
import type { WallCard } from "../api/apiClient";
import { makeWallCard } from "../fixtures";

vi.mock("../api/apiClient", () => ({ getWall: vi.fn() }));
vi.mock("../api/session", () => ({ useSession: () => ({ user: null, loading: false }) }));
import { getWall } from "../api/apiClient";

function card(id: string): WallCard {
  return makeWallCard({ id, title: `Toy ${id}`, heartCount: 0 });
}
const mockGetWall = getWall as ReturnType<typeof vi.fn>;
afterEach(() => { cleanup(); vi.clearAllMocks(); });

describe("Wall", () => {
  it("loads recent toys on mount and renders the grid", async () => {
    mockGetWall.mockResolvedValue({ toys: [card("a"), card("b")], nextPage: null });
    render(<MemoryRouter><Wall /></MemoryRouter>);
    expect(await screen.findByText("Toy a")).toBeInTheDocument();
    expect(screen.getByText("Toy b")).toBeInTheDocument();
    expect(mockGetWall).toHaveBeenCalledWith("recent", 0);
  });

  it("switching to popular refetches with sort=popular", async () => {
    mockGetWall.mockResolvedValue({ toys: [card("a")], nextPage: null });
    render(<MemoryRouter><Wall /></MemoryRouter>);
    await screen.findByText("Toy a");
    fireEvent.click(screen.getByRole("button", { name: /popular/i }));
    await waitFor(() => expect(mockGetWall).toHaveBeenCalledWith("popular", 0));
  });

  it("shows Load more only when nextPage is set, and appends the next page", async () => {
    mockGetWall
      .mockResolvedValueOnce({ toys: [card("a")], nextPage: 1 })
      .mockResolvedValueOnce({ toys: [card("b")], nextPage: null });
    render(<MemoryRouter><Wall /></MemoryRouter>);
    await screen.findByText("Toy a");
    fireEvent.click(screen.getByRole("button", { name: /load more/i }));
    expect(await screen.findByText("Toy b")).toBeInTheDocument();
    expect(mockGetWall).toHaveBeenLastCalledWith("recent", 1);
    await waitFor(() =>
      expect(screen.queryByRole("button", { name: /load more/i })).not.toBeInTheDocument(),
    );
  });

  it("shows an empty state when there are no toys", async () => {
    mockGetWall.mockResolvedValue({ toys: [], nextPage: null });
    render(<MemoryRouter><Wall /></MemoryRouter>);
    expect(await screen.findByText(/no toys yet/i)).toBeInTheDocument();
  });

  it("disables Load more while a page is in flight (no double-append)", async () => {
    // First page resolves; the load-more fetch hangs so we can observe the
    // pending guard.
    let releaseSecond: (v: { toys: WallCard[]; nextPage: number | null }) => void = () => {};
    mockGetWall
      .mockResolvedValueOnce({ toys: [card("a")], nextPage: 1 })
      .mockImplementationOnce(
        () => new Promise((resolve) => { releaseSecond = resolve; }),
      );
    render(<MemoryRouter><Wall /></MemoryRouter>);
    await screen.findByText("Toy a");

    const btn = screen.getByRole("button", { name: /load more/i });
    fireEvent.click(btn);
    // While pending, the button is disabled — a second click can't fire a
    // duplicate fetch.
    await waitFor(() => expect(screen.getByRole("button", { name: /loading/i })).toBeDisabled());
    fireEvent.click(screen.getByRole("button", { name: /loading/i }));
    expect(mockGetWall).toHaveBeenCalledTimes(2); // still just mount + one loadMore

    releaseSecond({ toys: [card("b")], nextPage: null });
    expect(await screen.findByText("Toy b")).toBeInTheDocument();
  });
});
