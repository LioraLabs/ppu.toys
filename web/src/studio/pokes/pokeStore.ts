import { useMemo } from "react";
import {
  openSketchStore,
  openContextFiles,
  useOpenSketch,
  type OpenSketchState,
} from "../sketches/openSketch";
import type { SketchFile } from "../sketches/sketchStore";
import { POKES_FILE, parsePokes, pokesToLua, upsertPoke, type Poke } from "./pokes";
import {
  evictCrossDialect,
  fieldPoke,
  regeneratePokes,
  type FieldWrite,
  type PokeDialect,
} from "../inspector/compose/model";
import { pokeDialect } from "../inspector/compose/dialect";
import {
  clampRange,
  formatKeyframes,
  pokeKeyframes,
  removeKeyframe,
  setKeyframe,
} from "./scanlinePokes";

/** The pokes.lua FILE is the source of truth — these helpers parse it out of the
 *  open context and write it back through editFile (autosave/fork/persist ride along). */

function pokesSource(files: readonly SketchFile[]): string {
  return files.find((f) => f.name === POKES_FILE)?.source ?? "";
}

export function currentPokes(s: OpenSketchState): Poke[] {
  return parsePokes(pokesSource(openContextFiles(s)));
}

export function usePokes(): Poke[] {
  const s = useOpenSketch();
  const src = pokesSource(openContextFiles(s));
  return useMemo(() => parsePokes(src), [src]);
}

/** Verbatim pokes.lua source of the open context — the PokeBar copy-function
 *  chip's store-sourced replacement for `Compositor.pokesSource`. */
export function usePokesSource(): string {
  return pokesSource(openContextFiles(useOpenSketch()));
}

/** Whether something outside pokes.lua calls apply_pokes() — store-sourced
 *  replacement for `Compositor.pokesApplied`. */
export function usePokesApplied(): boolean {
  return hasApplyCall(openContextFiles(useOpenSketch()));
}

function write(next: readonly Poke[]): void {
  openSketchStore.editFile(POKES_FILE, pokesToLua(next));
}

export function poke(p: Poke): void {
  pokeMany([p]);
}

/** Upsert a batch in ONE regeneration, first evicting the OTHER dialect's
 *  pokes on every register the batch touches (a raw CGADSUB = 0x80 must not
 *  coexist with a friendly color.op = "add" — the friendly fold would
 *  silently win). Covers every write path, including HexPoke's raw edits. */
export function pokeMany(ps: readonly Poke[]): void {
  const kept = evictCrossDialect(currentPokes(openSketchStore.state()), ps);
  write(ps.reduce((acc, p) => upsertPoke(acc, p), kept));
}

/** Upsert field writes as SCANLINE pokes: each write sets a keyframe on line
 *  `y` of its field, merged into whatever keyframes that field already has, in
 *  one regeneration. A field with no numeric value (a bool/string field like
 *  `screen.main.bg1`) has no curve to keyframe, so it falls back to its plain
 *  frame-wide poke — mixing the two in one batch is fine, they are different
 *  lvalues.
 *
 *  Cross-dialect eviction still applies: a scanline poke's lvalue is the
 *  friendly field, so writing one drops a raw whole-register poke on the same
 *  register exactly as a friendly write would. */
export function pokeKeyframeWrites(writes: readonly FieldWrite[], y: number): void {
  const existing = currentPokes(openSketchStore.state());
  const byLvalue = new Map(existing.map((p) => [p.lvalue, p]));
  const next = writes.map((w) => {
    const v = Number(w.expr);
    if (!Number.isFinite(v) || w.expr.trim() === "") return fieldPoke(w);
    const prior = byLvalue.get(w.field);
    const kf = prior ? (pokeKeyframes(prior) ?? []) : [];
    return {
      lvalue: w.field,
      expr: formatKeyframes(setKeyframe(kf, y, v)),
      note: `$${w.addr.toString(16).toUpperCase()} · per-scanline`,
      // keep whatever range the poke already had; a new one owns the frame
      ...(prior?.range ? { range: prior.range } : {}),
    };
  });
  pokeMany(next);
}

/** Rescope a scanline poke's hook to `[y0, y1]`. Outside that band the hook
 *  does not run, so the frame-wide value stands — this is what lets several
 *  pokes drive different parts of the screen independently. */
export function setKeyframeRange(lvalue: string, y0: number, y1: number): void {
  const p = currentPokes(openSketchStore.state()).find((x) => x.lvalue === lvalue);
  if (!p || !pokeKeyframes(p)) return;
  pokeMany([{ ...p, range: clampRange(y0, y1) }]);
}

/** Drop the keyframe on line `y` from `lvalue`; unpoke the field entirely when
 *  that was its last one (an empty keyframe list has no generated form). */
export function removeKeyframeAt(lvalue: string, y: number): void {
  const p = currentPokes(openSketchStore.state()).find((x) => x.lvalue === lvalue);
  const kf = p && pokeKeyframes(p);
  if (!kf) return;
  const next = removeKeyframe(kf, y);
  if (next.length === 0) unpoke(lvalue);
  else pokeMany([{ ...p, expr: formatKeyframes(next) }]);
}

export function unpoke(lvalue: string): void {
  write(currentPokes(openSketchStore.state()).filter((p) => p.lvalue !== lvalue));
}

export function unpokeMany(lvalues: readonly string[]): void {
  write(currentPokes(openSketchStore.state()).filter((p) => !lvalues.includes(p.lvalue)));
}

export function clearPokes(): void {
  write([]);
}

/** Flip the emission dialect AND rewrite every existing poke into it, in one
 *  regeneration. No-op when already in `d`. */
export function setDialect(d: PokeDialect): void {
  if (pokeDialect.get() === d) return;
  write(regeneratePokes(currentPokes(openSketchStore.state()), d));
  pokeDialect.set(d);
}

/** Token search outside pokes.lua. A commented-out call false-positives; accepted. */
export function hasApplyCall(files: readonly SketchFile[]): boolean {
  return files.some((f) => f.name !== POKES_FILE && /\bapply_pokes\b/.test(f.source));
}
