// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { AdminToyPicker } from "./AdminToyPicker";
import { getWall } from "../api/apiClient";
import { makeWallCard } from "../fixtures";
vi.mock("../api/apiClient", () => ({ getWall: vi.fn() }));
afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});

it("searches remotely, paginates, and reports a failed selection without changing it", async () => {
  vi.mocked(getWall).mockResolvedValue({
    toys: [makeWallCard({ id: "a", title: "Aurora" })],
    nextPage: 1,
  });
  const save = vi.fn().mockRejectedValue(new Error("offline"));
  render(<AdminToyPicker title="Toy of the Week" selected={[]} limit={1} onSave={save} />);
  fireEvent.click(screen.getByRole("button", { name: "Choose a toy" }));
  await screen.findByRole("button", { name: "Select Aurora" });
  fireEvent.change(screen.getByRole("searchbox"), { target: { value: "Aurora" } });
  await waitFor(() => expect(getWall).toHaveBeenCalledWith("recent", 0, "Aurora"));
  await screen.findByRole("button", { name: "Select Aurora" });
  fireEvent.click(screen.getByRole("button", { name: "Next" }));
  await waitFor(() => expect(getWall).toHaveBeenCalledWith("recent", 1, "Aurora"));
  fireEvent.click(await screen.findByRole("button", { name: "Select Aurora" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("Could not save selection");
  expect(save).toHaveBeenCalledWith([expect.objectContaining({ id: "a" })]);
  expect(screen.getByText("No toy selected.")).toBeInTheDocument();
});

it("caps highlights at five and preserves order when moving and removing toys", async () => {
  const selected = Array.from({ length: 5 }, (_, i) => ({
    id: `${i}`,
    title: `Toy ${i}`,
    author: "ann",
  }));
  vi.mocked(getWall).mockResolvedValue({
    toys: [makeWallCard({ id: "new", title: "New toy" })],
    nextPage: null,
  });
  const save = vi.fn().mockResolvedValue(undefined);
  render(
    <AdminToyPicker title="Community highlights" selected={selected} limit={5} onSave={save} />,
  );
  fireEvent.click(screen.getByRole("button", { name: "Find featured toys" }));
  expect(await screen.findByRole("button", { name: "Select New toy" })).toBeDisabled();
  fireEvent.click(screen.getByRole("button", { name: "Move Toy 1 up" }));
  await waitFor(() =>
    expect(save).toHaveBeenCalledWith([selected[1], selected[0], ...selected.slice(2)]),
  );
  await waitFor(() => expect(screen.getByRole("button", { name: "Remove Toy 0" })).toBeEnabled());
  fireEvent.click(screen.getByRole("button", { name: "Remove Toy 0" }));
  await waitFor(() => expect(save).toHaveBeenLastCalledWith(selected.slice(1)));
});

it("saves a weekly toy and returns keyboard focus to the search toggle", async () => {
  vi.mocked(getWall).mockResolvedValue({
    toys: [makeWallCard({ id: "a", title: "Aurora" })],
    nextPage: null,
  });
  const save = vi.fn().mockResolvedValue(undefined);
  render(<AdminToyPicker title="Toy of the Week" selected={[]} limit={1} onSave={save} />);
  fireEvent.click(screen.getByRole("button", { name: "Choose a toy" }));
  fireEvent.click(await screen.findByRole("button", { name: "Select Aurora" }));
  await waitFor(() => expect(screen.getByRole("button", { name: "Choose a toy" })).toHaveFocus());
  expect(save).toHaveBeenCalledWith([expect.objectContaining({ id: "a" })]);
});
