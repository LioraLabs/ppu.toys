import { describe, it, expect, beforeEach } from "vitest";
import { transport, Transport } from "./transport";
import type { PpuCore, FrameResult } from "../../ppu/core";

describe("transport store", () => {
  beforeEach(() => {
    transport.setPlaying(true);
    transport.seek(0);
  });

  it("getSnapshot is stable until something changes", () => {
    const a = transport.getSnapshot();
    expect(transport.getSnapshot()).toBe(a);
    transport.step(16);
    expect(transport.getSnapshot()).not.toBe(a);
  });

  it("step advances the clock and produces a frame", () => {
    transport.seek(0);
    const t0 = transport.getSnapshot().t;
    transport.step(100);
    const s = transport.getSnapshot();
    expect(s.t).toBeGreaterThan(t0);
    expect(s.frame.framebuffer.length).toBeGreaterThan(0);
  });

  it("setPlaying toggles play state and zeroes fps when paused", () => {
    transport.setPlaying(false);
    expect(transport.getSnapshot().playing).toBe(false);
    expect(transport.getSnapshot().fps).toBe(0);
    transport.setPlaying(true);
    expect(transport.getSnapshot().playing).toBe(true);
  });

  it("seeks to an absolute time", () => {
    transport.seek(12.5);
    expect(transport.getSnapshot().t).toBe(12.5);
  });

  it("setSources forwards to the core and refreshes the snapshot", () => {
    const before = transport.getSnapshot();
    const res = transport.setSources([
      { name: "util.lua", source: "function tint() return 5 end" },
      { name: "main.lua", source: "function frame() end" },
    ]);
    expect(res.ok).toBe(true);
    expect(transport.getSnapshot()).not.toBe(before);
  });

  it("setLayerVisible reaches the shared core (hiding bg1 darkens output)", () => {
    transport.seek(0.05);
    transport.setLayerVisible("bg1", false);
    const fb = transport.getSnapshot().frame.framebuffer;
    expect(fb[0]).toBe(0);
    transport.setLayerVisible("bg1", true);
  });
});

function fakeFrame(): FrameResult {
  return {
    framebuffer: new Uint8ClampedArray(4),
    registers: [],
    cgram: new Uint16Array(0),
    oam: [],
    objOverflow: { rangeOver: false, timeOver: false, maxSprites: 0, maxTiles: 0 },
  };
}

function makeCore(state: { throwing: boolean }): PpuCore {
  return {
    setSource: () => ({ ok: true }),
    setSources: () => ({ ok: true }),
    frame: () => {
      if (state.throwing)
        throw { message: "attempt to index a nil value", line: 3, file: "fx.lua" };
      return fakeFrame();
    },
    setLayerVisible: () => {},
    vram: () => new Uint16Array(0),
    importReports: () => [],
    screens: () => ({
      main: new Uint8ClampedArray(0),
      sub: new Uint8ClampedArray(0),
      mathMask: new Uint8Array(0),
    }),
    layerView: () => new Uint8ClampedArray(0),
    traceBgPixel: () => null,
    traceBgTile: () => null,
    traceObj: () => null,
    convertSource: () => ({
      payload: new Uint8Array(),
      meta: {
        width: 0,
        height: 0,
        report: {
          mode: "tile",
          report: {
            colors_used: 0,
            palettes_used: 0,
            tile_cells: 0,
            unique_tiles: 0,
            vram_words: 0,
            overflows: [],
          },
        },
      },
    }),
    addSource: () => ({ ok: true }),
    m7Scanlines: () => new Float32Array(0),
    m7MapView: () => new Uint8ClampedArray(0),
    winScanlines: () => new Uint8Array(0),
    removeSource: () => true,
  };
}

describe("transport runtime-error guard", () => {
  it("catches a thrown runtime error, surfaces it, and keeps the loop alive", () => {
    const state = { throwing: true };
    const tr = new Transport(() => makeCore(state));
    expect(() => tr.step(16)).not.toThrow();
    const err = tr.getSnapshot().runtimeError;
    expect(err?.message).toContain("nil value");
    expect(err?.line).toBe(3);
    expect(err?.file).toBe("fx.lua"); // per-file attribution survives the guard
  });

  it("keeps the same error object identity while the error is unchanged", () => {
    const tr = new Transport(() => makeCore({ throwing: true }));
    tr.step(16);
    const a = tr.getSnapshot().runtimeError;
    tr.step(16);
    expect(tr.getSnapshot().runtimeError).toBe(a);
  });

  it("clears runtimeError once frame() succeeds again", () => {
    const state = { throwing: true };
    const tr = new Transport(() => makeCore(state));
    tr.step(16);
    expect(tr.getSnapshot().runtimeError).toBeDefined();
    state.throwing = false;
    tr.step(16);
    expect(tr.getSnapshot().runtimeError).toBeUndefined();
  });
});

describe("transport multi-file recompile", () => {
  it("a successful setSources preserves the running clock (M9 contract)", () => {
    const tr = new Transport(() => makeCore({ throwing: false }));
    tr.step(100);
    tr.step(100);
    const before = tr.getSnapshot();
    tr.setSources([{ name: "main.lua", source: "function frame() end" }]);
    const after = tr.getSnapshot();
    expect(after.t).toBe(before.t);
    expect(after.f).toBe(before.f);
  });
});

describe("transport stop", () => {
  it("pauses and rewinds to t=0/f=0", () => {
    const tr = new Transport(() => makeCore({ throwing: false }));
    tr.step(100);
    tr.stop();
    expect(tr.getSnapshot()).toMatchObject({ t: 0, f: 0, playing: false, fps: 0 });
  });
});
