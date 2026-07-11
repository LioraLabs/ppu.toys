import { describe, it, expect } from "vitest";
import { decodeSourcePayload, quantizedRgba } from "./payload";

// hand-build v1 payloads per the locked byte layout
function u16le(v: number) { return [v & 0xff, (v >> 8) & 0xff]; }

describe("decodeSourcePayload", () => {
  it("returns null on bad version / truncation / stub", () => {
    expect(decodeSourcePayload(new Uint8Array([1]))).toBeNull();       // mock stub: no kind byte
    expect(decodeSourcePayload(new Uint8Array([2, 0]))).toBeNull();    // bad version
    expect(decodeSourcePayload(new Uint8Array())).toBeNull();          // transport fake
  });

  it("decodes a bg payload (2bpp, 1 tile, 1 pal, screen 0)", () => {
    const bytes = new Uint8Array([
      1, 0,             // version, kind=bg
      2, 8,             // bit_depth, tile_size
      1, 1, ...u16le(0x001f), // pal_count=1, pal0 len=1, color
      ...u16le(1),      // tile_count=1
      ...Array(8).fill(0).flatMap(() => u16le(0)), // 1 tile * (2*4)=8 words
      0,                // screen_size=0
      ...Array(0x400).fill(0).flatMap(() => u16le(0x0401)), // tilemap: tile1 pal1
    ]);
    const d = decodeSourcePayload(bytes);
    expect(d?.kind).toBe("bg");
    if (d?.kind !== "bg") throw new Error("kind");
    expect(d.bitDepth).toBe(2);
    expect(d.palettes).toEqual([[0x001f]]);
    expect(d.tiles.length).toBe(1);
    expect(d.tiles[0].length).toBe(64);
    // tilemap read for cell (0,0): tile=1, pal=1
    expect(d.tilemap[0]).toBe(0x0401);
  });

  it("decodes an m7 payload (2 tiles 2x1, flat pal)", () => {
    const bytes = new Uint8Array([
      1, 1,             // version, kind=m7
      0,                // opts_len
      2, ...u16le(0x001f), ...u16le(0x7fff), // pal_len=2 + colors
      ...u16le(2),      // tile_count=2
      ...Array(64).fill(7), ...Array(64).fill(3), // two chunky tiles
      2, 1,             // tiles_w, tiles_h
      0, 1,             // map
    ]);
    const d = decodeSourcePayload(bytes);
    if (d?.kind !== "m7") throw new Error("kind");
    expect(d.palette).toEqual([0x001f, 0x7fff]);
    expect(d.tilesW).toBe(2);
    expect(d.map).toEqual([0, 1]);
    expect(d.tiles[0][0]).toBe(7);
  });

  it("decodes an obj payload (1 tile 4bpp, 1 pal)", () => {
    const bytes = new Uint8Array([
      1, 2,             // version, kind=obj
      8,                // cell_size
      1, 1, ...u16le(0x03e0), // pal_count=1, pal0 len=1
      ...u16le(1),      // tile_count=1
      ...Array(16).fill(0).flatMap(() => u16le(0)), // 1 tile * 16 words
    ]);
    const d = decodeSourcePayload(bytes);
    if (d?.kind !== "obj") throw new Error("kind");
    expect(d.cellSize).toBe(8);
    expect(d.palettes).toEqual([[0x03e0]]);
    expect(d.tiles.length).toBe(1);
  });

  it("unpacks bitplanes: leftmost pixel = bit 7", () => {
    // 2bpp tile, row 0 word = plane0=0x80 -> pixel(0,0) index 1, rest 0
    const bytes = new Uint8Array([
      1, 0, 2, 8, 1, 1, ...u16le(0x001f), ...u16le(1),
      ...u16le(0x0080), ...Array(7).fill(0).flatMap(() => u16le(0)),
      0, ...Array(0x400).fill(0).flatMap(() => u16le(0)),
    ]);
    const d = decodeSourcePayload(bytes);
    if (d?.kind !== "bg") throw new Error("kind");
    expect(d.tiles[0][0]).toBe(1);
    expect(d.tiles[0][1]).toBe(0);
  });

  it("quantizedRgba paints a bg source to width*height*4 with alpha 0 transparent", () => {
    const bytes = new Uint8Array([
      1, 0, 2, 8, 1, 1, ...u16le(0x7c00), ...u16le(1),
      ...u16le(0x00ff), ...Array(7).fill(0).flatMap(() => u16le(0)), // row0 plane0 = all 8 px index 1
      0, ...Array(0x400).fill(0).flatMap(() => u16le(0x0000)),        // tile0 everywhere
    ]);
    const d = decodeSourcePayload(bytes)!;
    const { pixels, width, height } = quantizedRgba(d, 8, 8);
    expect(pixels.length).toBe(width * height * 4);
    // cell(0,0) tile0 -> row0 index1 -> opaque
    expect(pixels[3]).toBe(255); // first pixel opaque (index 1)
    // palette color 0x7c00 is pure blue in BGR555 -> r=0, b=255
    expect(pixels[0]).toBe(0);
    expect(pixels[2]).toBe(255);
  });

  it("quantizedRgba m7: chunky byte 0 transparent, byte i+1 -> palette[i]", () => {
    // 1 tile 1x1, palette[0]=blue(0x7c00) palette[1]=red(0x001f); tile pixel0=1 -> palette[0]
    const bytes = new Uint8Array([
      1, 1, 0, // version, kind=m7, opts_len=0
      2, ...u16le(0x7c00), ...u16le(0x001f), // pal_len=2
      ...u16le(1), // tile_count=1
      1, ...Array(63).fill(0), // tile: px0 index1 (=palette[0]), rest transparent
      1, 1, // tiles_w, tiles_h
      0,    // map -> tile 0
    ]);
    const d = decodeSourcePayload(bytes)!;
    const { pixels } = quantizedRgba(d, 8, 8);
    // px0 = byte 1 -> palette[0] = blue -> r=0,b=255, opaque
    expect([pixels[0], pixels[2], pixels[3]]).toEqual([0, 255, 255]);
    // px1 = byte 0 -> transparent
    expect(pixels[7]).toBe(0); // alpha of second pixel
  });
});
