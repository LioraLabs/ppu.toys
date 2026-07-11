import { cgram15ToCss } from "../inspector/format";

export interface DecodedBg {
  kind: "bg";
  bitDepth: 2 | 4 | 8;
  palettes: number[][]; // BGR555 per sub-palette; entry 0 (transparent) implicit
  tiles: number[][];    // 64 palette indices each (0 = transparent)
  screenSize: number;
  tilemap: number[];    // screen-ordered words
}
export interface DecodedM7 {
  kind: "m7";
  palette: number[];    // flat BGR555 (index 0 transparent)
  tiles: number[][];    // 64 chunky bytes each
  tilesW: number;
  tilesH: number;
  map: number[];        // tilesW*tilesH tile-number bytes
}
export interface DecodedObj {
  kind: "obj";
  cellSize: number;
  palettes: number[][];
  tiles: number[][];    // 64 idx each (4bpp)
}
export type Decoded = DecodedBg | DecodedM7 | DecodedObj;

class Rd {
  i = 0;
  constructor(private b: Uint8Array) {}
  private need(n: number) { if (this.i + n > this.b.length) throw new Error("eof"); }
  u8() { this.need(1); return this.b[this.i++]; }
  u16() { this.need(2); const v = this.b[this.i] | (this.b[this.i + 1] << 8); this.i += 2; return v; }
  u16s(n: number) { const o: number[] = []; for (let k = 0; k < n; k++) o.push(this.u16()); return o; }
  bytes(n: number) { this.need(n); const s = Array.from(this.b.subarray(this.i, this.i + n)); this.i += n; return s; }
  palettes() {
    const n = this.u8(); const out: number[][] = [];
    for (let k = 0; k < n; k++) { const len = this.u8(); out.push(this.u16s(len)); }
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

/** Decode a v1 source payload. Returns null on version!=1, unknown kind, or
 *  truncation (mock stub / transport fake) - callers degrade to source-image
 *  preview + budget only. */
export function decodeSourcePayload(bytes: Uint8Array): Decoded | null {
  try {
    const rd = new Rd(bytes);
    if (rd.u8() !== 1) return null;
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
      return { kind: "bg", bitDepth, palettes, tiles: unpackTiles(words, count, bitDepth), screenSize, tilemap };
    }
    if (kind === 1) {
      const optsLen = rd.u8(); rd.bytes(optsLen);
      const palLen = rd.u8();
      const palette = rd.u16s(palLen);
      const count = rd.u16();
      const tiles: number[][] = [];
      for (let t = 0; t < count; t++) tiles.push(rd.bytes(64));
      const tilesW = rd.u8();
      const tilesH = rd.u8();
      const map = rd.bytes(tilesW * tilesH);
      return { kind: "m7", palette, tiles, tilesW, tilesH, map };
    }
    if (kind === 2) {
      const cellSize = rd.u8();
      const palettes = rd.palettes();
      const count = rd.u16();
      const words = rd.u16s(count * 16);
      return { kind: "obj", cellSize, palettes, tiles: unpackTiles(words, count, 4) };
    }
    return null;
  } catch {
    return null;
  }
}

function rgbaFrom555(c: number): [number, number, number] {
  const css = cgram15ToCss(c); // "#rrggbb"
  return [parseInt(css.slice(1, 3), 16), parseInt(css.slice(3, 5), 16), parseInt(css.slice(5, 7), 16)];
}

/** Screen-order tilemap read for a bg source cell. */
export function bgCell(d: DecodedBg, cols: number, _rows: number, tx: number, ty: number) {
  const mapCols = cols > 32 ? 64 : 32;
  const sc = (ty >> 5) * (mapCols / 32) + (tx >> 5);
  const word = d.tilemap[sc * 0x400 + (ty % 32) * 32 + (tx % 32)] ?? 0;
  return { tile: word & 0x3ff, pal: (word >> 10) & 7, flipX: (word >> 14 & 1) === 1, flipY: (word >> 15 & 1) === 1 };
}

/** Paint a decoded source to an RGBA buffer (alpha 0 = transparent). width/height
 *  from meta. bg/m7 walk their tilemap/map; obj lays tiles row-major (base preview). */
export function quantizedRgba(d: Decoded, width: number, height: number): { pixels: Uint8ClampedArray; width: number; height: number } {
  const px = new Uint8ClampedArray(width * height * 4);
  const put = (x: number, y: number, rgb: [number, number, number] | null) => {
    if (x >= width || y >= height) return;
    const o = (y * width + x) * 4;
    if (!rgb) { px[o + 3] = 0; return; }
    px[o] = rgb[0]; px[o + 1] = rgb[1]; px[o + 2] = rgb[2]; px[o + 3] = 255;
  };
  if (d.kind === "bg") {
    const cols = Math.ceil(width / 8), rows = Math.ceil(height / 8);
    for (let ty = 0; ty < rows; ty++) for (let tx = 0; tx < cols; tx++) {
      const c = bgCell(d, cols, rows, tx, ty);
      const tile = d.tiles[c.tile] ?? d.tiles[0];
      const pal = d.palettes[c.pal] ?? [];
      for (let y = 0; y < 8; y++) for (let x = 0; x < 8; x++) {
        const sx = c.flipX ? 7 - x : x, sy = c.flipY ? 7 - y : y;
        const idx = tile?.[sy * 8 + sx] ?? 0;
        put(tx * 8 + x, ty * 8 + y, idx === 0 ? null : rgbaFrom555(pal[idx - 1] ?? 0));
      }
    }
  } else if (d.kind === "m7") {
    for (let ty = 0; ty < d.tilesH; ty++) for (let tx = 0; tx < d.tilesW; tx++) {
      const tile = d.tiles[d.map[ty * d.tilesW + tx] ?? 0] ?? [];
      for (let y = 0; y < 8; y++) for (let x = 0; x < 8; x++) {
        const b = tile[y * 8 + x] ?? 0;
        put(tx * 8 + x, ty * 8 + y, b === 0 ? null : rgbaFrom555(d.palette[b] ?? 0));
      }
    }
  } else {
    // obj: lay 8x8 char tiles row-major across the sheet (base preview; labels come from cells)
    const cols = Math.ceil(width / 8);
    d.tiles.forEach((tile, t) => {
      const tx = t % cols, ty = Math.floor(t / cols);
      const pal = d.palettes[0] ?? [];
      for (let y = 0; y < 8; y++) for (let x = 0; x < 8; x++) {
        const idx = tile[y * 8 + x] ?? 0;
        put(tx * 8 + x, ty * 8 + y, idx === 0 ? null : rgbaFrom555(pal[idx - 1] ?? 0));
      }
    });
  }
  return { pixels: px, width, height };
}
