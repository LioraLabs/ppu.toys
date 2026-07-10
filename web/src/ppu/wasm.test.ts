import { describe, it, expect } from "vitest";
import { wrapWasmCore, type WasmCoreLike } from "./wasm";
import type { ImportReport, SourceFile } from "./core";

function fakeCore(over: Partial<WasmCoreLike> = {}): WasmCoreLike {
  return {
    setSource: () => ({ ok: true }),
    frame: () => {},
    framebuffer: () => new Uint8ClampedArray(4),
    registers: () => [],
    cgram: () => new Uint16Array(0),
    vram: () => new Uint16Array(0),
    importReports: () => [],
    uploadTexture: () => {},
    setLayerVisible: () => {},
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

  it("falls back when older wasm glue has no VRAM/report methods", () => {
    const core = fakeCore();
    delete core.vram;
    delete core.importReports;
    const ppu = wrapWasmCore(core);
    expect(Array.from(ppu.vram())).toEqual([]);
    expect(ppu.importReports()).toEqual([]);
  });

  it("forwards objOverflow and falls back when the getter is absent", () => {
    const ov = { rangeOver: true, timeOver: false, maxSprites: 40, maxTiles: 34 };
    expect(wrapWasmCore(fakeCore({ objOverflow: () => ov })).frame(0, 0).objOverflow).toEqual(ov);
    expect(wrapWasmCore(fakeCore()).frame(0, 0).objOverflow).toEqual({
      rangeOver: false,
      timeOver: false,
      maxSprites: 0,
      maxTiles: 0,
    });
  });

  it("forwards setSources to the core when present", () => {
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

  it("setSources falls back to concatenated setSource on an older module", () => {
    const seen: string[] = [];
    const ppu = wrapWasmCore(
      fakeCore({
        setSource: (src: string) => {
          seen.push(src);
          return { ok: true };
        },
      }),
    );
    const res = ppu.setSources([
      { name: "a.lua", source: "x = 1" },
      { name: "b.lua", source: "y = 2" },
    ]);
    expect(res.ok).toBe(true);
    expect(seen).toEqual(["x = 1\ny = 2"]);
  });
});
