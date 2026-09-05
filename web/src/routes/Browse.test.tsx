// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { makeWallCard } from "../fixtures";
import { Browse } from "./Browse";

vi.mock("../api/apiClient", () => ({ getWall: vi.fn() }));
vi.mock("../api/session", () => ({ useSession: () => ({ user: null }) }));
import { getWall } from "../api/apiClient";

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

it("searches and sorts the toy archive", async () => {
  vi.mocked(getWall).mockResolvedValue({ toys: [makeWallCard({ title: "Road" })], nextPage: null });
  render(
    <MemoryRouter>
      <Browse />
    </MemoryRouter>,
  );
  await screen.findByText("Road");
  fireEvent.change(screen.getByRole("searchbox"), { target: { value: "mode7" } });
  fireEvent.submit(screen.getByRole("search"));
  await waitFor(() => expect(getWall).toHaveBeenCalledWith("recent", 0, "mode7", { tag: "" }));
  fireEvent.click(screen.getByRole("button", { name: "Popular" }));
  await waitFor(() => expect(getWall).toHaveBeenCalledWith("popular", 0, "mode7", { tag: "" }));
});

it("filters by linked tags and opens a feed with the same filter", async () => {
  vi.mocked(getWall).mockResolvedValue({
    toys: [makeWallCard({ tags: ["playable"] })],
    nextPage: null,
  });
  render(
    <MemoryRouter initialEntries={["/browse?tag=playable"]}>
      <Browse />
    </MemoryRouter>,
  );
  await screen.findByRole("link", { name: "#playable" });
  expect(getWall).toHaveBeenCalledWith("recent", 0, "", { tag: "playable" });
  expect(screen.getByRole("link", { name: "Play feed" })).toHaveAttribute(
    "href",
    "/t/abc123/play?tag=playable",
  );
});
