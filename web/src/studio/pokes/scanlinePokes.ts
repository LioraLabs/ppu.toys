/** The scanline poke dialect: a poke whose value varies down the frame.
 *
 *  A frame-wide poke is one assignment in `apply_pokes()`. A scanline poke is a
 *  keyframe list — `{{0,128},{104,52},{223,128}}` — that the generated file
 *  interpolates inside an `hdma()` hook, one assignment per scanline. The `Poke`
 *  record is unchanged: the keyframe list rides in `expr`, and an expr starting
 *  with `{` IS the discriminator (no scalar register value can be a table), so
 *  `upsertPoke`, `evictCrossDialect` and the poke chips all keep working on
 *  scanline pokes without knowing they exist.
 *
 *  `keyframeAt` mirrors the generated Lua `pk` helper exactly — the panel reads
 *  back a poked value through the same maths the engine will run. */

import type { Poke } from "./pokes";

export interface Keyframe {
  /** Scanline, 0..HEIGHT-1. */
  y: number;
  v: number;
}

/** Fields whose register takes a fractional value. Everything else is an
 *  integer register, and the DSL SILENTLY IGNORES a fractional write to one
 *  (`to_integer()` rejects a non-integral float), so the generated hook must
 *  round for those — see `pkHelper`. */
const FLOAT_LVALUES: ReadonlySet<string> = new Set(["m7.a", "m7.b", "m7.c", "m7.d"]);

/** Which generated helper interpolates this lvalue: `pki` rounds to an integer,
 *  `pkf` passes the raw lerp through. */
export function pkHelper(lvalue: string): "pki" | "pkf" {
  return FLOAT_LVALUES.has(lvalue) ? "pkf" : "pki";
}

/** Is this poke per-scanline? The expr carries a keyframe table, and nothing
 *  else in the poke vocabulary starts with `{`. */
export function isScanlinePoke(p: Poke): boolean {
  return p.expr.startsWith("{");
}

const NUM = String.raw`-?\d+(?:\.\d+)?`;
const PAIR = String.raw`\{${NUM},${NUM}\}`;
/** A whole keyframe expr, anchored — a partial or malformed table is rejected
 *  outright rather than half-parsed into a wrong curve. */
const KEYFRAMES = new RegExp(String.raw`^\{${PAIR}(?:,${PAIR})*\}$`);
const PAIR_G = new RegExp(String.raw`\{(${NUM}),(${NUM})\}`, "g");

/** Integers bare, fractions to 3dp with trailing zeros trimmed — the same rule
 *  the M7 and window snippets use, so generated numbers look alike everywhere. */
export function fmtNum(v: number): string {
  return Number.isInteger(v) ? String(v) : v.toFixed(3).replace(/0+$/, "").replace(/\.$/, "");
}

/** `{{0,128},{104,52}}`. Sorted by scanline and byte-stable — the file is the
 *  source of truth, so the same keyframes must always render identically. */
export function formatKeyframes(kf: readonly Keyframe[]): string {
  return `{${sortKeyframes(kf)
    .map((k) => `{${fmtNum(k.y)},${fmtNum(k.v)}}`)
    .join(",")}}`;
}

/** Parse a keyframe expr; null when it is not one (a scalar poke) or is
 *  malformed. */
export function parseKeyframes(expr: string): Keyframe[] | null {
  if (!KEYFRAMES.test(expr)) return null;
  const out: Keyframe[] = [];
  for (const m of expr.matchAll(PAIR_G)) out.push({ y: Number(m[1]), v: Number(m[2]) });
  return out.length > 0 ? out : null;
}

function sortKeyframes(kf: readonly Keyframe[]): Keyframe[] {
  return [...kf].sort((a, b) => a.y - b.y);
}

/** The value at scanline `y`. Mirrors the generated Lua `pk`: hold the first
 *  value before the first keyframe, hold the last after the last, interpolate
 *  linearly between. Rounding is NOT applied here — `keyframeValueAt` does that
 *  for integer lvalues, matching `pki`. */
export function keyframeAt(kf: readonly Keyframe[], y: number): number {
  const s = sortKeyframes(kf);
  if (s.length === 0) return 0;
  if (y <= s[0].y) return s[0].v;
  for (let i = 1; i < s.length; i++) {
    const a = s[i - 1];
    const b = s[i];
    if (y <= b.y) {
      const span = b.y - a.y;
      return span <= 0 ? b.v : a.v + ((b.v - a.v) * (y - a.y)) / span;
    }
  }
  return s[s.length - 1].v;
}

/** What the engine will actually put in the register at `y` — the interpolated
 *  value, rounded exactly as the generated `pki` rounds it. */
export function keyframeValueAt(lvalue: string, kf: readonly Keyframe[], y: number): number {
  const v = keyframeAt(kf, y);
  return pkHelper(lvalue) === "pki" ? Math.floor(v + 0.5) : v;
}

/** Set the value at scanline `y`, replacing a keyframe already on that line.
 *  This is what dragging an edge in scanline mode does: one keyframe per line
 *  you actually touch, the curve between them interpolated. */
export function setKeyframe(kf: readonly Keyframe[], y: number, v: number): Keyframe[] {
  return sortKeyframes([...kf.filter((k) => k.y !== y), { y, v }]);
}

/** Drop the keyframe on scanline `y` (no-op when there is none). Removing the
 *  last one is the caller's cue to unpoke the lvalue entirely — an empty
 *  keyframe list has no meaning in the generated file. */
export function removeKeyframe(kf: readonly Keyframe[], y: number): Keyframe[] {
  return sortKeyframes(kf.filter((k) => k.y !== y));
}

/** A scanline poke's keyframes, or null if it is a frame-wide poke. */
export function pokeKeyframes(p: Poke): Keyframe[] | null {
  return isScanlinePoke(p) ? parseKeyframes(p.expr) : null;
}

/** Last visible NTSC scanline. Hard-coded, not a knob. */
export const LAST_LINE = 223;

/** A poke with no explicit range owns the whole frame — the behaviour before
 *  ranges existed, so an older `pokes.lua` loads unchanged. */
export const DEFAULT_RANGE: readonly [number, number] = [0, LAST_LINE];

/** Clamp to the visible frame and keep the pair ordered, so a half-typed range
 *  can never generate an inverted `hdma(200, 30, …)` that silently covers
 *  nothing. */
export function clampRange(y0: number, y1: number): [number, number] {
  const lo = Math.min(Math.max(Math.round(y0) || 0, 0), LAST_LINE);
  const hi = Math.min(Math.max(Math.round(y1) || 0, 0), LAST_LINE);
  return lo <= hi ? [lo, hi] : [hi, lo];
}

/** The inclusive line range a scanline poke's hook covers. Outside it the hook
 *  does not run at all, so the frame-wide value — including a plain assignment
 *  in the script — stands. That is how a poke is scoped to a band. */
export function pokeRange(p: Poke): readonly [number, number] {
  return p.range ? clampRange(p.range[0], p.range[1]) : DEFAULT_RANGE;
}
