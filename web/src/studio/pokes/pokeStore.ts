import { useMemo } from "react";
import { openSketchStore, openContextFiles, useOpenSketch, type OpenSketchState } from "../sketches/openSketch";
import type { SketchFile } from "../sketches/sketchStore";
import { POKES_FILE, parsePokes, pokesToLua, upsertPoke, type Poke } from "./pokes";

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
  write(upsertPoke(currentPokes(openSketchStore.state()), p));
}

export function pokeMany(ps: readonly Poke[]): void {
  write(ps.reduce((acc, p) => upsertPoke(acc, p), currentPokes(openSketchStore.state())));
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

/** Token search outside pokes.lua. A commented-out call false-positives; accepted. */
export function hasApplyCall(files: readonly SketchFile[]): boolean {
  return files.some((f) => f.name !== POKES_FILE && /\bapply_pokes\b/.test(f.source));
}
