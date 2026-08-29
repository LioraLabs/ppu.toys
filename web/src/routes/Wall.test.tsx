// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { makeWallCard } from "../fixtures";
import { Wall } from "./Wall";

vi.mock("../api/apiClient", () => ({ getHighlights: vi.fn(), getWall: vi.fn() }));
vi.mock("../api/session", () => ({ useSession: () => ({ user: null }) }));
vi.mock("./hero/HeroTV", () => ({ default: () => null }));
import { getHighlights, getWall } from "../api/apiClient";

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  delete window.kofiWidgetOverlay;
});

it("shows curated highlights and the five latest contributions", async () => {
  const openKofi = vi.fn();
  const draw = vi.fn((_id, _config, containerId) => {
    const trigger = document.createElement("button");
    trigger.className = "floatingchat-donate-button";
    trigger.addEventListener("click", openKofi);
    document.getElementById(containerId!)?.appendChild(trigger);
  });
  window.kofiWidgetOverlay = {
    draw,
  };
  vi.mocked(getHighlights).mockResolvedValue({
    toys: [makeWallCard({ id: "featured", title: "Feature" })],
  });
  vi.mocked(getWall).mockResolvedValue({
    toys: Array.from({ length: 6 }, (_, i) => makeWallCard({ id: `${i}`, title: `Latest ${i}` })),
    nextPage: null,
  });
  render(
    <MemoryRouter>
      <Wall />
    </MemoryRouter>,
  );
  expect(await screen.findByText("Feature")).toBeInTheDocument();
  expect(await screen.findByText("Latest 4")).toBeInTheDocument();
  expect(screen.queryByText("Latest 5")).not.toBeInTheDocument();
  const script = document.querySelector(
    'script[src="https://storage.ko-fi.com/cdn/scripts/overlay-widget.js"]',
  )!;
  fireEvent.load(script);
  expect(draw).toHaveBeenCalledWith("X8X21XWLH3", expect.any(Object), "kofi-widget");
});
