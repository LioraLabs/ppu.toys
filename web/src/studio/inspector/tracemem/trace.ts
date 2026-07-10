import type { ObjTrace, PpuCore } from "../../../ppu/core";

/** Pure helpers for the Trace chain. No React, no DOM (node-env testable). */

export type TracePlane = "bg1" | "bg2" | "bg3" | "obj";
export const TRACE_PLANES: TracePlane[] = ["bg1", "bg2", "bg3", "obj"];

/** Packed BGR555 -> "#rrggbb" (same 5->8 bit expansion as format.cgram15ToCss). */
export function bgr555ToHex(c: number): string {
  const x = (v: number) => (((v << 3) | (v >> 2)) & 0xff).toString(16).padStart(2, "0");
  return `#${x(c & 0x1f)}${x((c >> 5) & 0x1f)}${x((c >> 10) & 0x1f)}`;
}

/** CGRAM index -> "CG $XX" (the click-to-copy label from the handoff). */
export function cgLabel(addr: number): string {
  return "CG $" + addr.toString(16).toUpperCase().padStart(2, "0");
}

/** BGR555 word -> "$XXXX". */
export function bgr555Label(word: number): string {
  return "$" + word.toString(16).toUpperCase().padStart(4, "0");
}

/** VRAM words a stored tile occupies: bpp/2 words per 8-px row, per 8x8 char. */
export function tileWords(bpp: number, w: number, h = w): number {
  return bpp * 4 * (w / 8) * (h / 8);
}

/** Direct-color BGR555 from an 8-bit index (BBGGGRRR; palette-word extension bits
 *  ignored — the trace pixel's exact bgr555 comes from the core when available). */
export function directColor555(index: number): number {
  const r = (index & 0x07) << 2;
  const g = ((index >> 3) & 0x07) << 2;
  const b = ((index >> 6) & 0x03) << 3;
  return (b << 10) | (g << 5) | r;
}

export interface PaletteEntry {
  cgAddr: number | null; // null = CGRAM bypassed (direct color)
  bgr555: number;
}

/** Resolve sub-palette entry `idx` at CGRAM `paletteBase` (direct color bypasses). */
export function resolvePaletteEntry(
  idx: number,
  paletteBase: number,
  cgram: ArrayLike<number>,
  directColor: boolean,
): PaletteEntry {
  if (directColor) return { cgAddr: null, bgr555: directColor555(idx) };
  const cgAddr = paletteBase + idx;
  return { cgAddr, bgr555: cgram[cgAddr] ?? 0 };
}

/** Stored (unflipped) tile/sprite indices -> RGBA bytes; index 0 = transparent. */
export function tileToRgba(
  pixels: number[],
  w: number,
  h: number,
  cgram: ArrayLike<number>,
  paletteBase: number,
  directColor: boolean,
): Uint8ClampedArray {
  const out = new Uint8ClampedArray(w * h * 4);
  const x8 = (v: number) => ((v << 3) | (v >> 2)) & 0xff;
  for (let i = 0; i < w * h; i++) {
    const p = pixels[i] ?? 0;
    if (p === 0) continue;
    const c = resolvePaletteEntry(p, paletteBase, cgram, directColor).bgr555;
    out[i * 4] = x8(c & 0x1f);
    out[i * 4 + 1] = x8((c >> 5) & 0x1f);
    out[i * 4 + 2] = x8((c >> 10) & 0x1f);
    out[i * 4 + 3] = 255;
  }
  return out;
}

/** One-line chain caption. Mode is REPORTED from the live frame (M9 deviation:
 *  scripts set the mode — possibly per scanline — so it is never a selector). */
export function traceCaption(plane: TracePlane, mode: number): string {
  return plane === "obj"
    ? `OBJ · mode ${mode} — OAM entry → char → sub-palette → color`
    : `${plane.toUpperCase()} · mode ${mode} — map entry → char → sub-palette → color`;
}

/** Front-most on-sprite whose box covers (x,y): lowest OAM index wins (SNES
 *  priority). 128 traceObj calls, click-time only. */
export function spriteAt(core: Pick<PpuCore, "traceObj">, x: number, y: number): ObjTrace | null {
  for (let i = 0; i < 128; i++) {
    const t = core.traceObj(i);
    if (!t || !t.oam.on) continue;
    if (x >= t.oam.x && x < t.oam.x + t.width && y >= t.oam.y && y < t.oam.y + t.height) return t;
  }
  return null;
}
