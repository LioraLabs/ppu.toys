// @vitest-environment jsdom
import type { ReactNode } from "react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { Link, MemoryRouter, Route, Routes } from "react-router-dom";
import { makeToyFull, makeWallCard } from "../fixtures";
import { Play } from "./Play";

vi.mock("../api/apiClient", () => ({
  getToy: vi.fn(),
  getWall: vi.fn(),
  addHeart: vi.fn(),
  removeHeart: vi.fn(),
  goToSignIn: vi.fn(),
}));
vi.mock("../api/session", () => ({
  useSession: () => ({ user: { id: "1", handle: "ada" } }),
  sessionStore: { refresh: vi.fn().mockResolvedValue(undefined) },
}));
vi.mock("../components/ReadOnlyPlayer", () => ({
  ReadOnlyPlayer: ({ children }: { children?: ReactNode }) => (
    <div className="player">
      <canvas aria-label="Game" />
      {children}
    </div>
  ),
}));
import { addHeart, removeHeart, getToy, getWall } from "../api/apiClient";
import { sessionStore } from "../api/session";

const card = (id: string) => makeWallCard({ id, title: id, tags: ["playable"] });
function open() {
  return render(
    <MemoryRouter initialEntries={["/t/one/play"]}>
      <Link to="/t/one/play?tag=playable">Filter playable</Link>
      <Link to="/t/one/play?author=ada">Filter mine</Link>
      <Routes>
        <Route path="/t/:id/play" element={<Play />} />
      </Routes>
    </MemoryRouter>,
  );
}
beforeEach(() => {
  vi.mocked(getToy).mockImplementation(async (id) =>
    makeToyFull({ id, title: id, heartCount: id === "one" ? 4 : 0, tags: ["playable"] }),
  );
  vi.mocked(getWall).mockResolvedValue({ toys: [card("one"), card("two")], nextPage: null });
});
afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

it("resolves direct-link sessions, hearts the current toy, paginates and goes back without leaking heart state", async () => {
  vi.mocked(getWall)
    .mockResolvedValueOnce({ toys: [card("one")], nextPage: 1 })
    .mockResolvedValueOnce({ toys: [card("one"), card("two")], nextPage: null });
  let finishHeart!: () => void;
  vi.mocked(addHeart).mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        finishHeart = resolve;
      }),
  );
  open();
  await screen.findByRole("heading", { name: "one" });
  expect(sessionStore.refresh).toHaveBeenCalled();
  expect(screen.getByRole("link", { name: "@ada" })).toHaveAttribute("href", "/u/ada");
  expect(screen.queryByRole("combobox")).not.toBeInTheDocument();
  const heart = screen.getByRole("button", { name: "Heart" });
  fireEvent.click(heart);
  fireEvent.click(heart);
  expect(addHeart).toHaveBeenCalledTimes(1);
  expect(addHeart).toHaveBeenCalledWith("one");
  expect(heart).toBeDisabled();
  finishHeart();
  await waitFor(() => expect(heart).not.toBeDisabled());
  fireEvent.click(screen.getByRole("button", { name: "Next toy" }));
  await screen.findByRole("heading", { name: "two" });
  expect(getWall).toHaveBeenCalledWith("recent", 1, "", { tag: "", author: "" });
  expect(screen.getByRole("button", { name: "Heart" })).toHaveTextContent("0");
  expect(screen.getByRole("button", { name: "Next toy" })).toBeDisabled();
  fireEvent.click(screen.getByRole("button", { name: "Previous toy" }));
  await screen.findByRole("heading", { name: "one" });
});

it("recovers feed and heart failures without pretending they succeeded", async () => {
  vi.mocked(getWall).mockRejectedValueOnce(new Error("offline"));
  vi.mocked(addHeart).mockRejectedValueOnce(new Error("offline"));
  open();
  await screen.findByRole("heading", { name: "one" });
  fireEvent.click(screen.getByRole("button", { name: "Heart" }));
  expect(await screen.findByText("Heart failed. Try again.")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Heart" })).toHaveTextContent("4");
  fireEvent.click(await screen.findByRole("button", { name: "Retry feed" }));
  await screen.findByRole("heading", { name: "two" });
});

it("ignores an old feed response after switching to playable or my toys", async () => {
  let finishOld!: (value: Awaited<ReturnType<typeof getWall>>) => void;
  vi.mocked(getWall).mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        finishOld = resolve;
      }),
  );
  vi.mocked(getWall).mockResolvedValue({ toys: [card("game")], nextPage: null });
  open();
  await screen.findByRole("heading", { name: "one" });
  fireEvent.click(screen.getByRole("link", { name: "Filter playable" }));
  await waitFor(() =>
    expect(getWall).toHaveBeenCalledWith("recent", 0, "", { tag: "playable", author: "" }),
  );
  finishOld({ toys: [card("wrong")], nextPage: null });
  await waitFor(() => expect(screen.getByRole("button", { name: "Next toy" })).not.toBeDisabled());
  fireEvent.click(screen.getByRole("button", { name: "Next toy" }));
  await screen.findByRole("heading", { name: "game" });
  expect(getToy).not.toHaveBeenCalledWith("wrong");
  fireEvent.click(screen.getByRole("link", { name: "Filter mine" }));
  await waitFor(() =>
    expect(getWall).toHaveBeenCalledWith("recent", 0, "", { tag: "", author: "ada" }),
  );
});

it("navigates on a vertical touch swipe over the canvas, with a downward swipe going back", async () => {
  open();
  await screen.findByRole("heading", { name: "one" });
  await waitFor(() => expect(screen.getByRole("button", { name: "Next toy" })).not.toBeDisabled());
  swipe(100, -70);
  expect(screen.getByRole("heading", { name: "one" })).toBeInTheDocument();
  swipe(0, -30);
  expect(screen.getByRole("heading", { name: "one" })).toBeInTheDocument();
  swipe(0, -90);
  await screen.findByRole("heading", { name: "two" });
  swipe(0, 90);
  await screen.findByRole("heading", { name: "one" });
});

function swipe(dx: number, dy: number) {
  const canvas = screen.getByLabelText("Game");
  (canvas.closest(".play-surface") as HTMLElement).setPointerCapture = vi.fn();
  for (const [type, x, y] of [
    ["pointerdown", 100, 150],
    ["pointerup", 100 + dx, 150 + dy],
  ] as const) {
    const event = new MouseEvent(type, { bubbles: true, clientX: x, clientY: y });
    Object.assign(event, { pointerId: 1, pointerType: "touch", isPrimary: true });
    fireEvent(canvas, event);
  }
}

it("a tap adds a heart once; swipes and controller taps do not heart", async () => {
  vi.mocked(addHeart).mockResolvedValue(undefined);
  const { container } = open();
  await screen.findByRole("heading", { name: "one" });
  swipe(0, 0);
  await waitFor(() => expect(addHeart).toHaveBeenCalledWith("one"));
  swipe(0, 0);
  expect(addHeart).toHaveBeenCalledTimes(1);
  expect(removeHeart).not.toHaveBeenCalled();
  swipe(0, -90);
  await screen.findByRole("heading", { name: "two" });
  expect(addHeart).toHaveBeenCalledTimes(1);
  const control = document.createElement("button");
  container.querySelector(".play-surface")!.appendChild(control);
  for (const type of ["pointerdown", "pointerup"]) {
    const event = new MouseEvent(type, { bubbles: true, button: 0 });
    Object.assign(event, { pointerId: 1, pointerType: "touch", isPrimary: true });
    fireEvent(control, event);
  }
  expect(addHeart).toHaveBeenCalledTimes(1);
});
