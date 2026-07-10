import type { OamSprite, RegisterView } from "../../../ppu/core";

/** Live VRAM/CGRAM ownership derivation. Regions come from the LIVE binding
 *  registers (M9 deviation — never the handoff's hardcoded table); usage
 *  extents from a scan of the bound tilemaps. Pure — node-env testable. */

/** bpp per BG (index 0..3) per supported mode — crates/ppu-core/src/modes.rs. */
export const MODE_BPP: Record<number, [number, number, number, number]> = {
  0: [2, 2, 2, 2],
  1: [4, 4, 2, 0],
  2: [4, 4, 0, 0],
  3: [8, 4, 0, 0],
  4: [8, 2, 0, 0],
  7: [8, 0, 0, 0],
};

export interface VramRegion {
  id: string; // "bg1-char" | "bg1-map" | ... | "obj-a" | "obj-b" | "m7"
  label: string; // "BG1 char"
  kind: "char" | "map"; // draw lane in the ownership bar
  start: number; // VRAM word address, inclusive
  end: number; // exclusive
  color: string;
  usage: string; // "6 tiles" | "32×32 map" | "interleaved map+char"
}

/** Region hues — handoff's palette, reused as fixed per-plane colors. */
export const REGION_COLORS: Record<string, string> = {
  "bg1-char": "#4a4f92", "bg1-map": "#3a4a8c",
  "bg2-char": "#a074c8", "bg2-map": "#7d55a6",
  "bg3-char": "#ff9ac0", "bg3-map": "#c86a92",
  "bg4-char": "#6ec9a8", "bg4-map": "#3f8f6f",
  "obj-a": "#ff9540", "obj-b": "#d97b2f",
  m7: "#4a4f92",
};

const MAP_WORDS = [0x400, 0x800, 0x800, 0x1000];
const MAP_DIMS = ["32×32", "64×32", "32×64", "64×64"];

function reg(registers: RegisterView[], name: string): number {
  return registers.find((r) => r.name === name)?.value ?? 0;
}

/** Highest tile name referenced by a bound tilemap (10-bit name field). */
function maxTileUsed(vram: Uint16Array, mapBase: number, mapWords: number): number {
  let max = 0;
  for (let i = 0; i < mapWords; i++) {
    const t = (vram[(mapBase + i) & 0x7fff] ?? 0) & 0x3ff;
    if (t > max) max = t;
  }
  return max;
}

export function vramRegions(registers: RegisterView[], vram: Uint16Array): VramRegion[] {
  const bgmode = reg(registers, "BGMODE");
  const mode = bgmode & 7;
  const bpp = MODE_BPP[mode] ?? MODE_BPP[1]; // modes 5/6 unsupported by the core -> mode-1 shape
  const out: VramRegion[] = [];

  if (mode === 7) {
    out.push({ id: "m7", label: "M7 map+char", kind: "char", start: 0, end: 0x4000, color: REGION_COLORS.m7, usage: "interleaved map+char" });
  } else {
    for (let i = 0; i < 4; i++) {
      if (!bpp[i]) continue;
      const sc = reg(registers, `BG${i + 1}SC`);
      const mapBase = ((sc >> 2) & 0x3f) << 10;
      const sizeBits = sc & 3;
      const mapWords = MAP_WORDS[sizeBits];
      const nba = reg(registers, i < 2 ? "BG12NBA" : "BG34NBA");
      const charBase = (i % 2 === 0 ? nba & 0x0f : (nba >> 4) & 0x0f) << 12;
      let maxTile = maxTileUsed(vram, mapBase, mapWords);
      if ((bgmode >> (4 + i)) & 1) maxTile = Math.min(maxTile + 17, 0x3ff); // 16px tiles span +17 names
      const tiles = maxTile + 1;
      out.push({
        id: `bg${i + 1}-char`, label: `BG${i + 1} char`, kind: "char",
        start: charBase, end: Math.min(charBase + tiles * bpp[i] * 4, 0x8000),
        color: REGION_COLORS[`bg${i + 1}-char`], usage: `${tiles} tiles`,
      });
      out.push({
        id: `bg${i + 1}-map`, label: `BG${i + 1} map`, kind: "map",
        start: mapBase, end: Math.min(mapBase + mapWords, 0x8000),
        color: REGION_COLORS[`bg${i + 1}-map`], usage: `${MAP_DIMS[sizeBits]} map`,
      });
    }
  }

  // OBJ: two 256-tile name tables from OBSEL (sprite.rs obj_tile_addr). Clamp
  // starts too — arbitrary OBSEL values (base bits >= 5) would otherwise yield
  // inverted regions the bar renderer would draw with negative width.
  const clampW = (v: number) => Math.min(v, 0x8000);
  const obsel = reg(registers, "OBSEL");
  const objBase = (obsel & 7) << 13;
  const gap = (((obsel >> 3) & 3) + 1) << 12;
  out.push({ id: "obj-a", label: "OBJ char A", kind: "char", start: clampW(objBase), end: clampW(objBase + 0x1000), color: REGION_COLORS["obj-a"], usage: "tiles 0–255" });
  out.push({ id: "obj-b", label: "OBJ char B", kind: "char", start: clampW(objBase + gap), end: clampW(objBase + gap + 0x1000), color: REGION_COLORS["obj-b"], usage: "tiles 256–511" });

  return out.sort((a, b) => a.start - b.start || a.end - b.end);
}

export interface CgramOwner {
  label: string; // "BG1 · BG2", "OBJ 3", "—"
  used: boolean; // drives dimming in the legend
}

/** 16 rows (16 colors each) labeled with the layers that live-bind them.
 *  BG rows: from the palette bits actually used in each bound tilemap
 *  (mode-0 2bpp band = bgIndex*32 — trace.rs). OBJ rows (8-15): always OBJ;
 *  `used` = a live on-sprite uses that pal OR a BG owner reaches the row.
 *  8bpp BG1 (modes 3/4/7) owns all 256 entries. */
export function cgramOwners(registers: RegisterView[], vram: Uint16Array, oam: OamSprite[]): CgramOwner[] {
  const owners: Set<string>[] = Array.from({ length: 16 }, () => new Set<string>());
  const bgmode = reg(registers, "BGMODE");
  const mode = bgmode & 7;
  const bpp = MODE_BPP[mode] ?? MODE_BPP[1]; // modes 5/6 unsupported by the core -> mode-1 shape

  if (bpp[0] === 8) {
    for (let row = 0; row < 16; row++) owners[row].add("BG1");
  }
  if (mode !== 7) {
    for (let i = 0; i < 4; i++) {
      if (!bpp[i] || bpp[i] === 8) continue;
      const sc = reg(registers, `BG${i + 1}SC`);
      const mapBase = ((sc >> 2) & 0x3f) << 10;
      const mapWords = MAP_WORDS[sc & 3];
      const pals = new Set<number>();
      for (let w = 0; w < mapWords; w++) pals.add(((vram[(mapBase + w) & 0x7fff] ?? 0) >> 10) & 7);
      const band = mode === 0 ? i * 32 : 0;
      const span = 1 << bpp[i];
      for (const p of pals) {
        const base = band + p * span;
        for (let row = base >> 4; row <= (base + span - 1) >> 4 && row < 16; row++) owners[row].add(`BG${i + 1}`);
      }
    }
  }

  return owners.map((set, row) => {
    if (row >= 8) {
      const pal = row - 8;
      const bgOwned = set.size > 0;
      set.add(`OBJ ${pal}`);
      const objUsed = oam.some((s) => s.on && s.pal === pal);
      return { label: [...set].join(" · "), used: objUsed || bgOwned };
    }
    return { label: set.size ? [...set].join(" · ") : "—", used: set.size > 0 };
  });
}
