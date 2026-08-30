import { describe, expect, it } from "vitest";
import {
  characterPokes,
  decodeTile2bpp,
  decodeTile4bpp,
  decodeTile8bpp,
  tilemapEntry,
} from "./VramTab";

describe("VRAM decoding helpers", () => {
  it("decodes planar 4bpp tile words", () => {
    const vram = new Uint16Array(0x8000);
    vram[0x1000] = 0x0080; // plane 0, row 0, left pixel
    vram[0x1000 + 8] = 0x8000; // plane 3, row 0, left pixel
    expect(decodeTile4bpp(vram, 0x1000, 0)[0]).toBe(9);
  });

  it("decodes planar 2bpp tile words", () => {
    const vram = new Uint16Array(0x8000);
    vram[0x2000] = 0x8080;
    expect(decodeTile2bpp(vram, 0x2000, 0)[0]).toBe(3);
  });

  it("decodes all four plane pairs of an 8bpp tile", () => {
    const vram = new Uint16Array(0x8000);
    vram[0] = 0x0080;
    vram[8] = 0x8000;
    vram[16] = 0x0080;
    vram[24] = 0x8000;
    expect(decodeTile8bpp(vram, 0, 0)[0]).toBe(0b10011001);
  });

  it("unpacks tilemap entry flags", () => {
    expect(tilemapEntry(5 | (3 << 10) | (1 << 13) | (1 << 14))).toEqual({
      tile: 5,
      pal: 3,
      prio: true,
      flipX: true,
      flipY: false,
    });
  });

  it("encodes an edited planar character as address-stable VRAM pokes", () => {
    const pixels = new Array(64).fill(0);
    pixels[0] = 9;
    const pokes = characterPokes(new Uint16Array(0x8000), 0x1000, 2, 4, pixels);
    expect(pokes).toHaveLength(16);
    expect(pokes[0]).toMatchObject({ lvalue: "vram[0x1020]", expr: "0x80" });
    expect(pokes[8]).toMatchObject({ lvalue: "vram[0x1028]", expr: "0x8000" });
  });

  it("preserves the Mode 7 tilemap byte while editing character pixels", () => {
    const vram = new Uint16Array(0x8000);
    vram[64] = 0x1234;
    const pixels = new Array(64).fill(0);
    pixels[0] = 0xab;
    expect(characterPokes(vram, 0, 1, 8, pixels, true)[0]).toMatchObject({
      lvalue: "vram[0x40]",
      expr: "0xab34",
    });
  });
});
