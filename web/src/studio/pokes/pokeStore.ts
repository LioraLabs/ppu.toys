import { useMemo } from "react";
import { openSketchStore, openContextFiles, useOpenSketch, type OpenSketchState } from "../sketches/openSketch";
import type { SketchFile } from "../sketches/sketchStore";
import { POKES_FILE, parsePokes, pokesToLua, upsertPoke, type Poke } from "./pokes";
import { evictCrossDialect, regeneratePokes, type PokeDialect } from "../inspector/compose/model";
import { pokeDialect } from "../inspector/compose/dialect";

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
