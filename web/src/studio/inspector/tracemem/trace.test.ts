import { describe, expect, it } from "vitest";
import {
  bgr555ToHex,
  bgr555Label,
  cgLabel,
  canvasPos,
  directColor555,
  resolvePaletteEntry,
  spriteAt,
  tileToRgba,
  tileWords,
  traceCaption,
} from "./trace";
import type { ObjTrace, OamSprite } from "../../../ppu/core";

describe("bgr555ToHex", () => {
  it("expands 5-bit channels like cgram15ToCss ((v<<3)|(v>>2))", () => {
    expect(bgr555ToHex(0x7fff)).toBe("#ffffff");
    expect(bgr555ToHex(0x0000)).toBe("#000000");
    expect(bgr555ToHex(0x001f)).toBe("#ff0000"); // low 5 bits = red
    expect(bgr555ToHex(0x7c00)).toBe("#0000ff"); // bits 10-14 = blue
  });
});

describe("labels", () => {
  it("formats CGRAM address + BGR555 word labels", () => {
    expect(cgLabel(0x4a)).toBe("CG $4A");
    expect(cgLabel(5)).toBe("CG $05");
    expect(bgr555Label(0x7fff)).toBe("$7FFF");
    expect(bgr555Label(0x1f)).toBe("$001F");
  });
});

describe("tileWords", () => {
  it("counts VRAM words per stored tile", () => {
    expect(tileWords(2, 8)).toBe(8);
    expect(tileWords(4, 8)).toBe(16);
    expect(tileWords(8, 8)).toBe(32);
    expect(tileWords(4, 16)).toBe(64); // 16px tile = 4 chars
    expect(tileWords(4, 32, 64)).toBe(512); // non-square OBJ 32x64
  });
});

describe("canvasPos", () => {
  const rect = { left: 10, top: 20, width: 512, height: 448 };
  it("maps client coords on a CSS-scaled canvas to source pixels", () => {
    expect(canvasPos(rect, 10, 20, 256, 224)).toEqual({ x: 0, y: 0 });
    expect(canvasPos(rect, 10 + 511, 20 + 447, 256, 224)).toEqual({ x: 255, y: 223 });
    expect(canvasPos(rect, 10 + 256, 20 + 224, 256, 224)).toEqual({ x: 128, y: 112 });
  });
  it("clamps outside coords into range", () => {
    expect(canvasPos(rect, 0, 0, 256, 224)).toEqual({ x: 0, y: 0 });
    expect(canvasPos(rect, 9999, 9999, 256, 224)).toEqual({ x: 255, y: 223 });
  });
});

describe("directColor555 + resolvePaletteEntry", () => {
  it("expands an 8-bit index to BGR555 (BBGGGRRR)", () => {
    expect(directColor555(0)).toBe(0);
    expect(directColor555(0x07)).toBe(0x1c); // rrr -> r5 bits 2-4
    expect(directColor555(0x38)).toBe(0x1c << 5); // ggg
    expect(directColor555(0xc0)).toBe(0x18 << 10); // bb
  });
  it("resolves a CGRAM entry", () => {
    const cgram = new Uint16Array(256);
    cgram[0x42] = 0x1234;
    expect(resolvePaletteEntry(2, 0x40, cgram, false)).toEqual({ cgAddr: 0x42, bgr555: 0x1234 });
  });
  it("bypasses CGRAM in direct-color mode", () => {
    const cgram = new Uint16Array(256);
    expect(resolvePaletteEntry(0x07, 0, cgram, true)).toEqual({ cgAddr: null, bgr555: 0x1c });
  });
});

describe("tileToRgba", () => {
  it("maps stored indices through the sub-palette; index 0 is transparent", () => {
    const cgram = new Uint16Array(256);
    cgram[17] = 0x001f; // pure red at base 16 + idx 1
    const rgba = tileToRgba([0, 1], 2, 1, cgram, 16, false);
    expect(Array.from(rgba.slice(0, 4))).toEqual([0, 0, 0, 0]); // idx 0 transparent
    expect(Array.from(rgba.slice(4, 8))).toEqual([255, 0, 0, 255]);
  });
});

describe("traceCaption", () => {
  it("mentions the plane and the reported mode", () => {
    expect(traceCaption("bg1", 3)).toContain("BG1");
    expect(traceCaption("bg1", 3)).toContain("mode 3");
    expect(traceCaption("obj", 1)).toContain("OBJ");
  });
});

describe("spriteAt", () => {
  const sprite = (over: Partial<OamSprite>): OamSprite => ({
    x: 0, y: 0, tile: 0, pal: 0, prio: 0, large: false, flipX: false, flipY: false, on: true,
    ...over,
  });
  const traced = new Map<number, ObjTrace>([
    [1, { index: 1, oam: sprite({ x: 10, y: 10 }), charBase: 0, charAddr: 0, width: 16, height: 16, pixels: [], paletteBase: 128, palette: [] }],
    [2, { index: 2, oam: sprite({ x: 12, y: 12 }), charBase: 0, charAddr: 0, width: 16, height: 16, pixels: [], paletteBase: 128, palette: [] }],
    [3, { index: 3, oam: sprite({ x: 200, y: 200, on: false }), charBase: 0, charAddr: 0, width: 16, height: 16, pixels: [], paletteBase: 128, palette: [] }],
  ]);
  const core = { traceObj: (i: number) => traced.get(i) ?? null };
  it("returns the front-most (lowest OAM index) on-sprite covering the point", () => {
    expect(spriteAt(core, 13, 13)?.index).toBe(1);
    expect(spriteAt(core, 27, 27)?.index).toBe(2); // only #2's box reaches here
  });
  it("ignores off sprites and misses", () => {
    expect(spriteAt(core, 201, 201)).toBeNull();
    expect(spriteAt(core, 250, 5)).toBeNull();
  });
});
