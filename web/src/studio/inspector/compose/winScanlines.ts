/** Per-scanline window state — the read side of `hdma()`-driven windows.
 *
 *  `ppuCore.winScanlines()` hands back the eleven window registers of every
 *  resolved scanline (`WIN_STRIDE` bytes per row, `crates/ppu-core/src/window.rs`
 *  `window_scanline_bytes`). This module is the lens over that buffer: read a
 *  register at a row, sweep the combined mask down the frame, spot which
 *  registers an hdma hook actually moves.
 *
 *  It is the window counterpart of the Mode-7 panel's `segments` fan, and like
 *  that fan it is READ-ONLY. The write path (pokes) stays frame-wide by
 *  construction — per-scanline authoring is Lua, which is what `winSnippet.ts`
 *  generates. */

import { HEIGHT, WIDTH, WIN_STRIDE } from "../../../ppu/core";
import { REG, columnMask, type ReadReg, type WinLogic, type WindowBounds } from "./model";

/** The registers a `winScanlines()` row carries, in buffer order. Mirrors
 *  `WIN_SCANLINE_STRIDE`'s documented layout in the Rust core. */
export const WIN_SCANLINE_REGS: readonly number[] = [
  REG.WH0,
  REG.WH1,
  REG.WH2,
  REG.WH3,
  REG.W12SEL,
  REG.W34SEL,
  REG.WOBJSEL,
  REG.WBGLOG,
  REG.WOBJLOG,
  REG.TMW,
  REG.TSW,
];

/** How many scanlines a buffer actually carries (0 before the first frame). */
export function rowCount(rows: Uint8Array): number {
  return Math.floor(rows.length / WIN_STRIDE);
}

/** Clamp a scanline index into the buffer; -1 when there are no rows. */
function clampRow(rows: Uint8Array, y: number): number {
  const n = rowCount(rows);
  return n === 0 ? -1 : Math.min(Math.max(y | 0, 0), n - 1);
}

/** `addr`'s value on scanline `y`, or null when the buffer is empty or `addr`
 *  is not one of the eleven window registers (callers fall back to the frame's
 *  row-0 snapshot for those). */
export function regAt(rows: Uint8Array, y: number, addr: number): number | null {
  const i = WIN_SCANLINE_REGS.indexOf(addr);
  const r = clampRow(rows, y);
  return i < 0 || r < 0 ? null : rows[r * WIN_STRIDE + i];
}

/** A `ReadReg` pinned to scanline `y`: window registers come from that row,
 *  everything else (TM/TS, the CGWSEL family) falls through to `base`. This is
 *  the "scanline lens" the Windows sections read through, so every existing
 *  control reports row `y` without knowing the feed exists. */
export function readAt(rows: Uint8Array, y: number, base: ReadReg): ReadReg {
  return (addr) => regAt(rows, y, addr) ?? base(addr);
}

/** WH0-3 on scanline `y`. */
export function boundsAt(rows: Uint8Array, y: number, base: ReadReg): WindowBounds {
  const at = readAt(rows, y, base);
  return { wh0: at(REG.WH0), wh1: at(REG.WH1), wh2: at(REG.WH2), wh3: at(REG.WH3) };
}

/** Does `addr` change anywhere in the frame? This is what earns a register its
 *  HDMA chip — and the warning that a frame-wide poke to it will be overwritten
 *  by the hook on every line the hook covers. */
export function variesAcrossFrame(rows: Uint8Array, addr: number): boolean {
  const i = WIN_SCANLINE_REGS.indexOf(addr);
  const n = rowCount(rows);
  if (i < 0 || n < 2) return false;
  const first = rows[i];
  for (let y = 1; y < n; y++) {
    if (rows[y * WIN_STRIDE + i] !== first) return true;
  }
  return false;
}

/** Every window register the frame sweeps. Empty = a static window, and the
 *  panel is then showing exactly what the row-0 readout already said. */
export function varyingRegs(rows: Uint8Array): number[] {
  return WIN_SCANLINE_REGS.filter((addr) => variesAcrossFrame(rows, addr));
}

/** What the Windows sections read the frame through: the feed, the selected
 *  scanline, and which registers sweep. Built once per render in the tab so the
 *  varying scan (11 registers x 224 rows) runs once, not per section. */
export interface WinSweep {
  rows: Uint8Array;
  /** The scanline every control and readout reports. */
  y: number;
  /** Window registers an `hdma()` hook moves across this frame. */
  varying: ReadonlySet<number>;
}

export function makeSweep(rows: Uint8Array, y: number): WinSweep {
  return { rows, y, varying: new Set(varyingRegs(rows)) };
}

/** One WH register traced down the frame: its value on every scanline. The
 *  preview draws these as polylines, so a swept edge reads as a curve instead
 *  of the straight line a row-0 snapshot implies. */
export function edgeTrace(rows: Uint8Array, addr: number, base: ReadReg): number[] {
  const n = rowCount(rows);
  if (n === 0) return new Array<number>(HEIGHT).fill(base(addr));
  const i = WIN_SCANLINE_REGS.indexOf(addr);
  const out = new Array<number>(n);
  for (let y = 0; y < n; y++) out[y] = rows[y * WIN_STRIDE + i];
  return out;
}

/** The combined W1/W2 mask for the WHOLE frame: `WIDTH * HEIGHT` bytes, 1 =
 *  inside. Rows past the feed reuse the last resolved row, so a short buffer
 *  degrades to the static mask rather than punching a hole in the preview.
 *  `logic`/`outside` come from the layer-agnostic combine + area controls, the
 *  same pair `columnMask` takes — only the bounds vary per row. */
export function sweptMask(
  rows: Uint8Array,
  base: ReadReg,
  logic: WinLogic,
  outside: boolean,
): Uint8Array {
  const mask = new Uint8Array(WIDTH * HEIGHT);
  const n = rowCount(rows);
  let prev = "";
  let row: Uint8Array = new Uint8Array(WIDTH);
  for (let y = 0; y < HEIGHT; y++) {
    const b = boundsAt(rows, Math.min(y, Math.max(0, n - 1)), base);
    // Consecutive scanlines usually share bounds (a hook covers a band, or the
    // window is static) — recompute the 256-wide row only when they move.
    const key = `${b.wh0},${b.wh1},${b.wh2},${b.wh3}`;
    if (key !== prev) {
      row = columnMask(b, logic, outside);
      prev = key;
    }
    mask.set(row, y * WIDTH);
  }
  return mask;
}

/** Handoff dimming outside a FULL-FRAME mask (x0.3 R/G, x0.42 B) — the
 *  per-scanline twin of `dimOutsideMask`, which applies one column mask to
 *  every row. */
export function dimOutsideSweptMask(fb: Uint8ClampedArray, mask: Uint8Array): Uint8ClampedArray {
  const out = new Uint8ClampedArray(fb.length);
  for (let p = 0; p < WIDTH * HEIGHT; p++) {
    const i = p * 4;
    if (mask[p]) {
      out[i] = fb[i];
      out[i + 1] = fb[i + 1];
      out[i + 2] = fb[i + 2];
    } else {
      out[i] = fb[i] * 0.3;
      out[i + 1] = fb[i + 1] * 0.3;
      out[i + 2] = fb[i + 2] * 0.42;
    }
    out[i + 3] = 255;
  }
  return out;
}
