import { useSyncExternalStore } from "react";
import type { PokeDialect } from "./model";

/** Persisted studio preference: which dialect NEW pokes emit — friendly field
 *  lines (`color.op = "sub"`) or raw whole-register mnemonics
 *  (`CGADSUB = 0x41`). Emission-only: loading stays dialect-agnostic
 *  (parsePokes reads both), existing lines are never rewritten. */

export const DIALECT_STORAGE_KEY = "ppu.toys:poke-dialect";

/** Normalize an untrusted stored value. Friendly is the default. */
export function parseDialect(raw: unknown): PokeDialect {
  return raw === "raw" ? "raw" : "friendly";
}

/** Load the persisted dialect (SSR/no-storage safe). */
export function loadDialect(): PokeDialect {
  try {
    return parseDialect(localStorage.getItem(DIALECT_STORAGE_KEY));
  } catch {
    return "friendly"; // storage unavailable (private mode / node)
  }
}

const listeners = new Set<() => void>();
let current: PokeDialect = loadDialect();

/** Shared external store: useCompositor reads it at write time, the
 *  DialectToggle subscribes for rendering. */
export const pokeDialect = {
  get: (): PokeDialect => current,
  set(next: PokeDialect): void {
    if (next === current) return;
    current = next;
    try {
      localStorage.setItem(DIALECT_STORAGE_KEY, next);
    } catch {
      /* non-persistent is fine */
    }
    for (const l of listeners) l();
  },
  subscribe(cb: () => void): () => void {
    listeners.add(cb);
    return () => void listeners.delete(cb);
  },
};

export const usePokeDialect = (): PokeDialect =>
  useSyncExternalStore(pokeDialect.subscribe, pokeDialect.get);
