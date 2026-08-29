import type { ObjCellMeta } from "../../ppu/core";

export interface DecodedBg {
  kind: "bg";
  bitDepth: 2 | 4 | 8;
  palettes: number[][]; // BGR555 per sub-palette; entry 0 (transparent) implicit
  tiles: number[][]; // 64 palette indices each (0 = transparent)
  screenSize: number;
  tilemap: number[]; // screen-ordered words
}
export interface DecodedM7 {
  kind: "m7";
  extbg?: boolean;
  palette: number[]; // flat BGR555, 0-based; chunky byte 0 = transparent, byte i+1 = palette[i]
  tiles: number[][]; // 64 chunky bytes each
  tilesW: number;
  tilesH: number;
  map: number[]; // tilesW*tilesH tile-number bytes
}
export interface DecodedObj {
  kind: "obj";
  cellSize: number;
  palettes: number[][];
  tiles: number[][]; // 64 idx each (4bpp)
  cells?: ObjCellMeta[];
}
/** Tilesheet: chars in row-major sheet order, no tilemap and no reserved blank
 *  — tile N is the Nth 8x8 cell of the source PNG. */
export interface DecodedSheet {
  kind: "sheet";
  bitDepth: 2 | 4 | 8;
  palettes: number[][];
  tiles: number[][]; // 64 palette indices each (0 = transparent)
}
export type Decoded = DecodedBg | DecodedM7 | DecodedObj | DecodedSheet;

class Rd {
  i = 0;
  constructor(private b: Uint8Array) {}
  private need(n: number) {
    if (this.i + n > this.b.length) throw new Error("eof");
  }
  u8() {
    this.need(1);
    return this.b[this.i++];
  }
  u16() {
    this.need(2);
    const v = this.b[this.i] | (this.b[this.i + 1] << 8);
    this.i += 2;
    return v;
  }
  u16s(n: number) {
    const o: number[] = [];
    for (let k = 0; k < n; k++) o.push(this.u16());
    return o;
  }
  bytes(n: number) {
    this.need(n);
    const s = Array.from(this.b.subarray(this.i, this.i + n));
    this.i += n;
    return s;
  }
  palettes() {
    const n = this.u8();
    const out: number[][] = [];
    for (let k = 0; k < n; k++) {
      const len = this.u8();
      out.push(this.u16s(len));
    }
    return out;
  }
}

function unpackTiles(words: number[], count: number, bpp: number): number[][] {
  const wpt = bpp * 4;
  const tiles: number[][] = [];
  for (let t = 0; t < count; t++) {
    const base = t * wpt;
    const px = new Array<number>(64).fill(0);
    for (let r = 0; r < 8; r++) {
      for (let x = 0; x < 8; x++) {
        let v = 0;
        for (let p = 0; p < bpp; p++) {
          const w = words[base + (p >> 1) * 8 + r] ?? 0;
          v |= ((w >> ((p & 1 ? 8 : 0) + (7 - x))) & 1) << p;
        }
        px[r * 8 + x] = v;
      }
    }
    tiles.push(px);
  }
  return tiles;
}

/** Decode a v1/v2 source payload. Returns null on an unknown version/kind or
 *  truncation (mock stub / transport fake) - callers degrade to source-image
 *  preview + budget only. */
export function decodeSourcePayload(bytes: Uint8Array): Decoded | null {
  try {
    const rd = new Rd(bytes);
    const version = rd.u8();
    if (version !== 1 && version !== 2) return null;
    const kind = rd.u8();
    if (kind === 0) {
      const bitDepth = rd.u8() as 2 | 4 | 8;
      rd.u8(); // tile_size
      const palettes = rd.palettes();
      const count = rd.u16();
      const words = rd.u16s(count * bitDepth * 4);
      const screenSize = rd.u8();
      const nScreens = screenSize === 0 ? 1 : screenSize === 3 ? 4 : 2;
      const tilemap = rd.u16s(nScreens * 0x400);
      return {
        kind: "bg",
        bitDepth,
        palettes,
        tiles: unpackTiles(words, count, bitDepth),
        screenSize,
        tilemap,
      };
    }
    if (kind === 1) {
      const optsLen = rd.u8();
      const opts = rd.bytes(optsLen);
      const extbg = opts[0] === 1;
      const palLen = rd.u8();
      const palette = rd.u16s(palLen);
      const count = rd.u16();
      const tiles: number[][] = [];
      for (let t = 0; t < count; t++) tiles.push(rd.bytes(64));
      const tilesW = rd.u8();
      const tilesH = rd.u8();
      const map = rd.bytes(tilesW * tilesH);
      return { kind: "m7", extbg, palette, tiles, tilesW, tilesH, map };
    }
    if (kind === 2) {
      const cellSize = rd.u8();
      const palettes = rd.palettes();
      const count = rd.u16();
      const words = rd.u16s(count * 16);
      const cells: ObjCellMeta[] = [];
      if (version >= 2) {
        const cellCount = rd.u16();
        for (let i = 0; i < cellCount; i++) {
          const tile = rd.u16(),
            pal = rd.u8(),
            flags = rd.u8();
          cells.push({ tile, pal, flip_x: (flags & 1) !== 0, flip_y: (flags & 2) !== 0 });
        }
      }
      return { kind: "obj", cellSize, palettes, tiles: unpackTiles(words, count, 4), cells };
    }
    if (kind === 3) {
      // sheet: bit depth, palettes, char words. No tile_size, no screen size,
      // no tilemap — the author owns the map geometry.
      const bitDepth = rd.u8() as 2 | 4 | 8;
      const palettes = rd.palettes();
      const count = rd.u16();
      const words = rd.u16s(count * bitDepth * 4);
      return { kind: "sheet", bitDepth, palettes, tiles: unpackTiles(words, count, bitDepth) };
    }
    return null;
  } catch {
    return null;
  }
}

export function rgbaFrom555(c: number): [number, number, number] {
  const x = (v: number) => (v << 3) | (v >> 2); // 5-bit -> 8-bit, matches cgram15ToCss
  return [x(c & 0x1f), x((c >> 5) & 0x1f), x((c >> 10) & 0x1f)];
}

/** Screen-order tilemap read for a bg source cell. */
export function bgCell(d: DecodedBg, cols: number, _rows: number, tx: number, ty: number) {
  const mapCols = cols > 32 ? 64 : 32;
  const sc = (ty >> 5) * (mapCols / 32) + (tx >> 5);
  const word = d.tilemap[sc * 0x400 + (ty % 32) * 32 + (tx % 32)] ?? 0;
  return {
    tile: word & 0x3ff,
    pal: (word >> 10) & 7,
    flipX: ((word >> 14) & 1) === 1,
    flipY: ((word >> 15) & 1) === 1,
  };
}

/** Paint a decoded source to an RGBA buffer (alpha 0 = transparent). width/height
 *  from meta. bg/m7 walk their tilemap/map. obj REASSEMBLES the sheet from
 *  `cells` (tile#/pal/flips per source cell; block cells stride the name table
 *  +1 right, +16 down like obj_tile_addr). sheet — and obj without cells —
 *  paint the row-major 8px tile atlas, which for a sheet IS the source image:
 *  tile N is the Nth PNG cell, in `cells[N].pal`. */
export function quantizedRgba(
  d: Decoded,
  width: number,
  height: number,
  cells?: ObjCellMeta[],
): { pixels: Uint8ClampedArray; width: number; height: number } {
  if (d.kind === "obj" && (!cells || cells.length === 0)) cells = d.cells;
  const px = new Uint8ClampedArray(width * height * 4);
  const put = (x: number, y: number, rgb: [number, number, number] | null) => {
    if (x >= width || y >= height) return;
    const o = (y * width + x) * 4;
    if (!rgb) {
      px[o + 3] = 0;
      return;
    }
    px[o] = rgb[0];
    px[o + 1] = rgb[1];
    px[o + 2] = rgb[2];
    px[o + 3] = 255;
  };
  if (d.kind === "bg") {
    const cols = Math.ceil(width / 8),
      rows = Math.ceil(height / 8);
    for (let ty = 0; ty < rows; ty++)
      for (let tx = 0; tx < cols; tx++) {
        const c = bgCell(d, cols, rows, tx, ty);
        const tile = d.tiles[c.tile] ?? d.tiles[0];
        const pal = d.palettes[c.pal] ?? [];
        for (let y = 0; y < 8; y++)
          for (let x = 0; x < 8; x++) {
            const sx = c.flipX ? 7 - x : x,
              sy = c.flipY ? 7 - y : y;
            const idx = tile?.[sy * 8 + sx] ?? 0;
            put(tx * 8 + x, ty * 8 + y, idx === 0 ? null : rgbaFrom555(pal[idx - 1] ?? 0));
          }
      }
  } else if (d.kind === "m7") {
    for (let ty = 0; ty < d.tilesH; ty++)
      for (let tx = 0; tx < d.tilesW; tx++) {
        const tile = d.tiles[d.map[ty * d.tilesW + tx] ?? 0] ?? [];
        for (let y = 0; y < 8; y++)
          for (let x = 0; x < 8; x++) {
            const b = tile[y * 8 + x] ?? 0;
            const color = d.extbg ? b & 0x7f : b;
            put(
              tx * 8 + x,
              ty * 8 + y,
              color === 0 ? null : rgbaFrom555(d.palette[color - 1] ?? 0),
            );
          }
      }
  } else if (d.kind === "obj" && cells && cells.length) {
    // obj: rebuild the source sheet — each cell places its (deduped, possibly
    // flipped) tiles back where they came from, with the cell's sub-palette.
    // Gated on the kind: a sheet also carries `cells`, but it has no cellSize
    // and its chars already sit in source order.
    const n = Math.max(1, d.cellSize / 8); // 8x8 tiles per cell edge
    const cols = Math.max(1, Math.ceil(width / d.cellSize));
    cells.forEach((c, k) => {
      const cx = (k % cols) * d.cellSize,
        cy = Math.floor(k / cols) * d.cellSize;
      const pal = d.palettes[c.pal] ?? [];
      for (let sy = 0; sy < n; sy++)
        for (let sx = 0; sx < n; sx++) {
          // cell 8: tile is the cell. Blocks: base + name-table stride.
          const tile = d.tiles[d.cellSize === 8 ? c.tile : c.tile + sy * 16 + sx];
          if (!tile) continue;
          for (let y = 0; y < 8; y++)
            for (let x = 0; x < 8; x++) {
              const fx = c.flip_x ? 7 - x : x,
                fy = c.flip_y ? 7 - y : y;
              const idx = tile[fy * 8 + fx] ?? 0;
              put(
                cx + sx * 8 + x,
                cy + sy * 8 + y,
                idx === 0 ? null : rgbaFrom555(pal[idx - 1] ?? 0),
              );
            }
        }
    });
  } else {
    // sheet (tile N = the Nth source cell, in its own sub-palette), and obj
    // without cells (mock/degraded, everything in sub-palette 0).
    const cols = Math.ceil(width / 8);
    d.tiles.forEach((tile, t) => {
      const tx = t % cols,
        ty = Math.floor(t / cols);
      const pal = d.palettes[cells?.[t]?.pal ?? 0] ?? [];
      for (let y = 0; y < 8; y++)
        for (let x = 0; x < 8; x++) {
          const idx = tile[y * 8 + x] ?? 0;
          put(tx * 8 + x, ty * 8 + y, idx === 0 ? null : rgbaFrom555(pal[idx - 1] ?? 0));
        }
    });
  }
  return { pixels: px, width, height };
}
