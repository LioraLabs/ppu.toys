/** IndexedDB-backed sketch persistence. A Sketch is the unit of user work:
 *  ordered Lua files plus format-committed graphics sources (raw bytes —
 *  IndexedDB's structured clone stores Uint8Array natively). All mutators fire
 *  onSketchesChanged so the library panel can refresh its list. */

import type { SourceKind, ConvertSourceOptions, SourceMeta } from "../../ppu/core";

export interface SketchFile {
  name: string;
  source: string;
}

/** A format-committed graphics source: the versioned payload from convertSource
 *  plus its authoring metadata. This — not raw RGBA — is what saves/forks/syncs. */
export interface SketchSource {
  name: string;
  kind: SourceKind;
  options: ConvertSourceOptions;
  payload: Uint8Array;
  meta: SourceMeta;
}

export interface Sketch {
  id: string;
  name: string;
  createdAt: number;
  updatedAt: number;
  /** Ordered: becomes chunk execution order when multi-file lands (M9). */
  files: SketchFile[];
  sources: SketchSource[];
  /** Demo id this sketch was lazily forked from, if any. Restoring a forked
   *  sketch re-runs that demo's procedural assets instead of storing copies. */
  forkedFrom?: string;
}

/** What the library list shows — everything but the payloads. */
export type SketchMeta = Omit<Sketch, "files" | "sources">;

const DB_NAME = "ppu-toys";
const STORE = "sketches";

let dbPromise: Promise<IDBDatabase> | null = null;

function openDb(): Promise<IDBDatabase> {
  if (!dbPromise) {
    dbPromise = new Promise((resolve, reject) => {
      const req = indexedDB.open(DB_NAME, 1);
      req.onupgradeneeded = () => req.result.createObjectStore(STORE, { keyPath: "id" });
      req.onsuccess = () => resolve(req.result);
      req.onerror = () => {
        dbPromise = null; // don't cache the failure — allow a retry next call
        reject(req.error);
      };
    });
  }
  return dbPromise;
}

/** Run one operation in its own transaction; resolve with the request result. */
async function withStore<T>(
  mode: IDBTransactionMode,
  op: (s: IDBObjectStore) => IDBRequest<T>,
): Promise<T> {
  const db = await openDb();
  return new Promise<T>((resolve, reject) => {
    const tx = db.transaction(STORE, mode);
    const req = op(tx.objectStore(STORE));
    tx.oncomplete = () => resolve(req.result);
    tx.onerror = () => reject(tx.error);
    tx.onabort = () => reject(tx.error);
  });
}

// ── change notification (library panel refresh) ─────────────────────────────
const listeners = new Set<() => void>();
function emit() {
  for (const l of listeners) l();
}

export function onSketchesChanged(cb: () => void): () => void {
  listeners.add(cb);
  return () => void listeners.delete(cb);
}

// ── CRUD ────────────────────────────────────────────────────────────────────

/** Build a fresh (unpersisted) Sketch. openSketch.ts uses this to fork a demo
 *  synchronously; persistence rides the autosave flush. */
export function newSketchObject(
  name: string,
  files: SketchFile[],
  sources: SketchSource[] = [],
  forkedFrom?: string,
): Sketch {
  const now = Date.now();
  return {
    id: crypto.randomUUID(),
    name,
    createdAt: now,
    updatedAt: now,
    files,
    sources,
    ...(forkedFrom ? { forkedFrom } : {}),
  };
}

export async function createSketch(
  name: string,
  files: SketchFile[],
  sources: SketchSource[] = [],
  forkedFrom?: string,
): Promise<Sketch> {
  const sketch = newSketchObject(name, files, sources, forkedFrom);
  await withStore("readwrite", (s) => s.put(sketch));
  emit();
  return sketch;
}

/** Upsert (also first-persists a lazily-forked sketch); bumps updatedAt. */
export async function saveSketch(sketch: Sketch): Promise<Sketch> {
  const stored = { ...sketch, updatedAt: Date.now() };
  await withStore("readwrite", (s) => s.put(stored));
  emit();
  return stored;
}

/** One-way migration: legacy sketches carried raw-PNG `assets`. Drop them (no
 *  auto-quantize — depth is unknowable without the deleted bind context) and
 *  default `sources`. On next saveSketch the record is rewritten clean. */
function normalize(raw: Sketch & { assets?: unknown }): Sketch {
  const { assets: _legacy, ...rest } = raw;
  return { ...rest, sources: rest.sources ?? [] };
}

export async function loadSketch(id: string): Promise<Sketch | undefined> {
  const raw = await withStore<(Sketch & { assets?: unknown }) | undefined>(
    "readonly",
    (s) => s.get(id),
  );
  return raw ? normalize(raw) : undefined;
}

export async function listSketches(): Promise<SketchMeta[]> {
  const all = await withStore<(Sketch & { assets?: unknown })[]>("readonly", (s) => s.getAll());
  return all
    .map(({ files: _f, sources: _s, assets: _a, ...meta }) => meta)
    .sort((a, b) => b.updatedAt - a.updatedAt);
}

export async function renameSketch(id: string, name: string): Promise<void> {
  const sketch = await loadSketch(id);
  if (!sketch) return;
  await saveSketch({ ...sketch, name });
}

export async function duplicateSketch(id: string): Promise<Sketch | undefined> {
  const sketch = await loadSketch(id);
  if (!sketch) return undefined;
  return createSketch(`${sketch.name} (copy)`, sketch.files, sketch.sources, sketch.forkedFrom);
}

export async function deleteSketch(id: string): Promise<void> {
  await withStore("readwrite", (s) => s.delete(id));
  emit();
}

/** Test hook: drop the cached connection + listeners so a fresh fake
 *  IndexedDB (new IDBFactory per test) takes effect. */
export function _resetSketchStoreForTests(): void {
  void dbPromise?.then((db) => db.close()).catch(() => undefined);
  dbPromise = null;
  listeners.clear();
}
