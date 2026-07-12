// @vitest-environment jsdom
import { describe, it, expect, afterEach, vi } from "vitest";
import "@testing-library/jest-dom/vitest";
import { render, cleanup } from "@testing-library/react";

// vi.mock factories are hoisted above top-level code, so any variable they
// reference must be created via vi.hoisted (plain `const`s declared above
// vi.mock would still be in the TDZ when the factory runs).
const { setSources, addSource, getSnapshot, subscribe } = vi.hoisted(() => ({
  setSources: vi.fn(() => ({ ok: true })),
  addSource: vi.fn(() => ({ ok: true })),
  getSnapshot: vi.fn(() => ({ frame: { framebuffer: new Uint8ClampedArray(256 * 224 * 4) } })),
  subscribe: vi.fn(() => () => {}),
}));
vi.mock("../studio/transport/transport", () => ({
  transport: { setSources, addSource, getSnapshot, subscribe },
}));
// Presenter touches WebGL; stub it — this test asserts wiring, not pixels.
// NOTE: a plain constructor-function + prototype is used here (not an ES
// `class { field = vi.fn() }` expression) — under this vitest/esbuild combo,
// a class-field-initialized mock instantiated from inside a real React effect
// flush (i.e. actually mounted via render()) trips a bogus
// "Cannot access '__vi_import_N__' before initialization" at module load.
// The constructor-function form is behaviorally identical and avoids it.
const { init } = vi.hoisted(() => ({ init: vi.fn(() => true) }));
vi.mock("../studio/output/presenter", () => {
  function Presenter() {}
  Presenter.prototype.init = init;
  Presenter.prototype.resize = vi.fn();
  Presenter.prototype.render = vi.fn();
  Presenter.prototype.dispose = vi.fn();
  return { Presenter };
});

import { ReadOnlyPlayer } from "./ReadOnlyPlayer";

// jsdom has no ResizeObserver; the component observes its container to
// integer-scale the canvas on resize.
vi.stubGlobal("ResizeObserver", class { observe() {} disconnect() {} unobserve() {} });

afterEach(() => { cleanup(); vi.clearAllMocks(); });

describe("ReadOnlyPlayer", () => {
  const files = [{ name: "main.lua", source: "-- toy" }];
  const sources = [{ name: "sky", payload: new Uint8Array([1, 2, 3]) }];

  it("pushes files then each source into the shared transport on mount", () => {
    render(<ReadOnlyPlayer files={files} sources={sources} />);
    expect(setSources).toHaveBeenCalledWith(files);
    expect(addSource).toHaveBeenCalledWith("sky", sources[0].payload);
  });

  it("renders a canvas and no editing controls", () => {
    const { container } = render(<ReadOnlyPlayer files={files} sources={[]} />);
    expect(container.querySelector("canvas")).toBeInTheDocument();
    expect(container.querySelector("input[type=range]")).toBeNull();
  });
});
