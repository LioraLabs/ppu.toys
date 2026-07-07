import { describe, expect, it } from "vitest";
import { decodeTile2bpp, decodeTile4bpp, tilemapEntry } from "./VramTab";

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

  it("unpacks tilemap entry flags", () => {
    expect(tilemapEntry(5 | (3 << 10) | (1 << 13) | (1 << 14))).toEqual({
      tile: 5,
      pal: 3,
      prio: true,
      flipX: true,
      flipY: false,
    });
  });
});
