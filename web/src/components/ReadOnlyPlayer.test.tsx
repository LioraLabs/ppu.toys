// @vitest-environment jsdom
import { describe, it, expect, afterEach, beforeEach, vi } from "vitest";
import "@testing-library/jest-dom/vitest";
import { render, cleanup, fireEvent } from "@testing-library/react";

// vi.mock factories are hoisted above top-level code, so any variable they
// reference must be created via vi.hoisted (plain `const`s declared above
// vi.mock would still be in the TDZ when the factory runs).
const {
  setSources,
  addSource,
  removeSource,
  getSnapshot,
  subscribe,
  toggle,
  setPlaying,
  seek,
  setPad,
} = vi.hoisted(() => ({
  setSources: vi.fn(() => ({ ok: true })),
  addSource: vi.fn(() => ({ ok: true })),
  removeSource: vi.fn(() => true),
  getSnapshot: vi.fn(() => ({
    playing: true,
    frame: { framebuffer: new Uint8ClampedArray(256 * 224 * 4) },
  })),
  subscribe: vi.fn(() => () => {}),
  toggle: vi.fn(),
  setPlaying: vi.fn(),
  seek: vi.fn(),
  setPad: vi.fn(),
}));
vi.mock("../studio/transport/transport", () => ({
  transport: {
    setSources,
    addSource,
    removeSource,
    getSnapshot,
    subscribe,
    toggle,
    setPlaying,
    seek,
    setPad,
  },
  useTransport: () => getSnapshot(),
}));
// Presenter touches WebGL; stub it — this test asserts wiring, not pixels.
// NOTE: a plain constructor-function + prototype is used here (not an ES
// `class { field = vi.fn() }` expression) — under this vitest/esbuild combo,
// a class-field-initialized mock instantiated from inside a real React effect
// flush (i.e. actually mounted via render()) trips a bogus
// "Cannot access '__vi_import_N__' before initialization" at module load.
// The constructor-function form is behaviorally identical and avoids it.
const { init, presentRender, resize } = vi.hoisted(() => ({
  init: vi.fn(() => true),
  presentRender: vi.fn(),
  resize: vi.fn(),
}));
vi.mock("../studio/output/presenter", () => {
  function Presenter() {}
  Presenter.prototype.init = init;
  Presenter.prototype.resize = resize;
  Presenter.prototype.render = presentRender;
  Presenter.prototype.dispose = vi.fn();
  return { Presenter };
});

import { ReadOnlyPlayer } from "./ReadOnlyPlayer";

// jsdom has no ResizeObserver; the component observes its container to
// integer-scale the canvas on resize.
vi.stubGlobal(
  "ResizeObserver",
  class {
    observe() {}
    disconnect() {}
    unobserve() {}
  },
);

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

beforeEach(() => {
  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    value: vi.fn(() => ({ matches: false })),
  });
});

describe("ReadOnlyPlayer", () => {
  const files = [{ name: "main.lua", source: "-- toy" }];
  const sources = [{ name: "sky", payload: new Uint8Array([1, 2, 3]) }];

  it("pushes sources BEFORE files — setup-stage dma() only sees registered sources", () => {
    render(<ReadOnlyPlayer files={files} sources={sources} />);
    expect(setSources).toHaveBeenCalledWith(files);
    expect(addSource).toHaveBeenCalledWith("sky", sources[0].payload);
    expect(addSource.mock.invocationCallOrder[0]).toBeLessThan(
      setSources.mock.invocationCallOrder[0],
    );
  });

  it("renders a canvas and no editing controls", () => {
    const { container } = render(<ReadOnlyPlayer files={files} sources={[]} />);
    expect(container.querySelector("canvas")).toBeInTheDocument();
    expect(container.querySelector("input[type=range]")).toBeNull();
    expect(container.querySelector("button")).toHaveTextContent("Pause");
  });

  it("CRT filter is on by default and toggles off with a repaint", () => {
    const { getByRole } = render(<ReadOnlyPlayer files={files} sources={[]} />);
    const btn = getByRole("button", { name: "CRT" });
    expect(btn).toHaveAttribute("aria-pressed", "true");
    expect(presentRender).toHaveBeenLastCalledWith(
      expect.anything(),
      expect.objectContaining({ crt: true }),
    );
    fireEvent.click(btn);
    expect(btn).toHaveAttribute("aria-pressed", "false");
    expect(presentRender).toHaveBeenLastCalledWith(
      expect.anything(),
      expect.objectContaining({ crt: false }),
    );
  });

  it("starts paused when reduced motion is requested", () => {
    vi.spyOn(window, "matchMedia").mockReturnValue({ matches: true } as MediaQueryList);
    render(<ReadOnlyPlayer files={files} sources={[]} />);
    expect(setPlaying).toHaveBeenCalledWith(false);
  });

  it("keeps raw output at native resolution with no controls or CRT pass", () => {
    const { container } = render(<ReadOnlyPlayer files={files} sources={[]} raw />);
    expect(container.querySelector(".player--raw")).toBeInTheDocument();
    expect(container.querySelector("button")).toBeNull();
    expect(resize).toHaveBeenCalledWith(1);
    expect(presentRender).toHaveBeenLastCalledWith(
      expect.anything(),
      expect.objectContaining({ crt: false }),
    );

    fireEvent.keyDown(window, { code: "Space" });
    fireEvent.keyDown(window, { code: "KeyR" });
    expect(toggle).toHaveBeenCalledOnce();
    expect(seek).toHaveBeenCalledWith(0);
  });
});

it("combines controller and keyboard input and clears it on unmount", () => {
  const { container, getByRole, unmount } = render(
    <ReadOnlyPlayer files={[]} sources={[]} controls />,
  );
  const frame = container.querySelector(".player")!;
  fireEvent.keyDown(frame, { code: "ArrowUp" });
  fireEvent.keyDown(getByRole("button", { name: "A" }), { key: " " });
  expect(setPad).toHaveBeenLastCalledWith(1 | 16);
  fireEvent.keyUp(frame, { code: "ArrowUp" });
  expect(setPad).toHaveBeenLastCalledWith(16);
  unmount();
  expect(setPad).toHaveBeenLastCalledWith(0);
});
