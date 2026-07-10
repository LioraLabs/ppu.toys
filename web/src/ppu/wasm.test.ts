import { describe, it, expect } from "vitest";
import { wrapWasmCore, type WasmCoreLike } from "./wasm";
import type { ImportReport, SourceFile } from "./core";
import type { BgTrace } from "./core";

function fakeCore(over: Partial<WasmCoreLike> = {}): WasmCoreLike {
  return {
    setSource: () => ({ ok: true }),
    setSources: () => ({ ok: true }),
    frame: () => {},
    framebuffer: () => new Uint8ClampedArray(4),
    registers: () => [],
    cgram: () => new Uint16Array(0),
    vram: () => new Uint16Array(0),
    oam: () => [],
    objOverflow: () => ({ rangeOver: false, timeOver: false, maxSprites: 0, maxTiles: 0 }),
    listAssets: () => [],
    importReports: () => [],
    uploadTexture: () => {},
    setLayerVisible: () => {},
    mainScreen: () => new Uint8Array(0),
    subScreen: () => new Uint8Array(0),
    mathMask: () => new Uint8Array(0),
    layerView: () => new Uint8Array(0),
    traceBgPixel: () => null,
    traceBgTile: () => null,
    traceObj: () => null,
    pinRegister: () => {},
    unpinRegister: () => {},
    clearPins: () => {},
    listPins: () => [],
    ...over,
  };
}

describe("wrapWasmCore", () => {
  it("assembles a FrameResult from the core getters on success", () => {
    const ppu = wrapWasmCore(fakeCore());
    const fr = ppu.frame(0, 0);
    expect(fr.framebuffer.length).toBe(4);
    expect(fr.oam).toEqual([]);
  });

  it("propagates a Lua runtime error thrown by core.frame", () => {
    const err = { message: "attempt to index a nil value (global 'nope')", line: 3 };
    const ppu = wrapWasmCore(
      fakeCore({
        frame: () => {
          throw err;
        },
      }),
    );
    expect(() => ppu.frame(0, 0)).toThrow();
    try {
      ppu.frame(0, 0);
    } catch (e) {
      expect((e as { message: string }).message).toContain("nil value");
      expect((e as { line: number }).line).toBe(3);
    }
  });

  it("forwards live VRAM words and import reports", () => {
    const reports: ImportReport[] = [
      {
        mode: "tile",
        layer: 0,
        report: {
          colors_used: 2,
          palettes_used: 1,
          tile_cells: 1,
          unique_tiles: 1,
          vram_words: 17,
          overflows: [],
        },
      },
    ];
    const ppu = wrapWasmCore(
      fakeCore({
        vram: () => new Uint16Array([0x1234, 0xabcd]),
        importReports: () => reports,
      }),
    );
    expect(Array.from(ppu.vram())).toEqual([0x1234, 0xabcd]);
    expect(ppu.importReports()).toEqual(reports);
  });

  it("forwards objOverflow into the FrameResult", () => {
    const ov = { rangeOver: true, timeOver: false, maxSprites: 40, maxTiles: 34 };
    expect(wrapWasmCore(fakeCore({ objOverflow: () => ov })).frame(0, 0).objOverflow).toEqual(ov);
  });

  it("forwards setSources to the core", () => {
    const seen: SourceFile[][] = [];
    const ppu = wrapWasmCore(
      fakeCore({
        setSources: (files) => {
          seen.push(files);
          return { ok: false, error: { message: "boom", line: 2, file: "util.lua" } };
        },
      }),
    );
    const files = [{ name: "util.lua", source: "x = = 1" }];
    const res = ppu.setSources(files);
    expect(seen).toEqual([files]);
    expect(res.ok).toBe(false);
    expect(res.error?.file).toBe("util.lua");
  });
});

describe("wrapWasmCore view seams", () => {
  it("assembles screens() from the three intermediate getters", () => {
    const ppu = wrapWasmCore(
      fakeCore({
        mainScreen: () => new Uint8Array([1, 2, 3, 255]),
        subScreen: () => new Uint8Array([4, 5, 6, 255]),
        mathMask: () => new Uint8Array([1]),
      }),
    );
    const s = ppu.screens();
    expect(Array.from(s.main)).toEqual([1, 2, 3, 255]);
    expect(Array.from(s.sub)).toEqual([4, 5, 6, 255]);
    expect(Array.from(s.mathMask)).toEqual([1]);
  });

  it("forwards layerView as a clamped copy", () => {
    const buf = new Uint8Array(4).fill(9);
    expect(Array.from(wrapWasmCore(fakeCore({ layerView: () => buf })).layerView("bg1"))).toEqual([
      9, 9, 9, 9,
    ]);
  });

  it("forwards trace queries and normalizes an undefined result to null", () => {
    const trace = { regs: { mode: 1 } } as unknown as BgTrace;
    const ppu = wrapWasmCore(fakeCore({ traceBgPixel: () => trace }));
    expect(ppu.traceBgPixel(1, 0, 0)).toBe(trace);
    // wasm-bindgen serializes a Rust `None` as undefined; the seam contract is null
    const none = wrapWasmCore(
      fakeCore({
        traceBgPixel: () => undefined,
        traceBgTile: () => undefined,
        traceObj: () => undefined,
      }),
    );
    expect(none.traceBgPixel(1, 0, 0)).toBeNull();
    expect(none.traceBgTile(1, 0, 0, 0)).toBeNull();
    expect(none.traceObj(0)).toBeNull();
  });

  it("forwards pin calls and lists pins", () => {
    const calls: unknown[] = [];
    const ppu = wrapWasmCore(
      fakeCore({
        pinRegister: (a: number, v: number) => calls.push(["pin", a, v]),
        unpinRegister: (a: number) => calls.push(["unpin", a]),
        clearPins: () => calls.push(["clear"]),
        listPins: () => [{ addr: 0x2100, value: 7 }],
      }),
    );
    ppu.pin(0x2100, 7);
    ppu.unpin(0x2100);
    ppu.clearPins();
    expect(calls).toEqual([["pin", 0x2100, 7], ["unpin", 0x2100], ["clear"]]);
    expect(ppu.listPins()).toEqual([{ addr: 0x2100, value: 7 }]);
  });
});
