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
  await waitFor(() => expect(getWall).toHaveBeenCalledWith("recent", 0, "mode7"));
  fireEvent.click(screen.getByRole("button", { name: "Popular" }));
  await waitFor(() => expect(getWall).toHaveBeenCalledWith("popular", 0, "mode7"));
});
