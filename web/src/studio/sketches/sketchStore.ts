/** IndexedDB-backed sketch persistence. A Sketch is the unit of user work:
 *  ordered Lua files plus uploaded PNG assets (raw bytes — IndexedDB's
 *  structured clone stores Uint8Array natively). All mutators fire
 *  onSketchesChanged so the library panel can refresh its list. */

export interface SketchFile {
  name: string;
  source: string;
}

/** An uploaded PNG, stored as the original file bytes. The runtime asset id is
 *  re-minted deterministically on restore (see restore.ts). */
export interface SketchAsset {
  name: string;
  png: Uint8Array;
}

export interface Sketch {
  id: string;
  name: string;
  createdAt: number;
  updatedAt: number;
  /** Ordered: becomes chunk execution order when multi-file lands (M9). */
  files: SketchFile[];
  assets: SketchAsset[];
  /** Demo id this sketch was lazily forked from, if any. Restoring a forked
   *  sketch re-runs that demo's procedural assets instead of storing copies. */
  forkedFrom?: string;
}

/** What the library list shows — everything but the payloads. */
export type SketchMeta = Omit<Sketch, "files" | "assets">;

const DB_NAME = "ppu-toys";
const STORE = "sketches";

let dbPromise: Promise<IDBDatabase> | null = null;

function openDb(): Promise<IDBDatabase> {
  if (!dbPromise) {
    dbPromise = new Promise((resolve, reject) => {
      const req = indexedDB.open(DB_NAME, 1);
      req.onupgradeneeded = () => req.result.createObjectStore(STORE, { keyPath: "id" });
      req.onsuccess = () => resolve(req.result);
      req.onerror = () => reject(req.error);
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
  assets: SketchAsset[] = [],
  forkedFrom?: string,
): Sketch {
  const now = Date.now();
  return {
    id: crypto.randomUUID(),
    name,
    createdAt: now,
    updatedAt: now,
    files,
    assets,
    ...(forkedFrom ? { forkedFrom } : {}),
  };
}

export async function createSketch(
  name: string,
  files: SketchFile[],
  assets: SketchAsset[] = [],
  forkedFrom?: string,
): Promise<Sketch> {
  const sketch = newSketchObject(name, files, assets, forkedFrom);
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

export async function loadSketch(id: string): Promise<Sketch | undefined> {
  return withStore<Sketch | undefined>("readonly", (s) => s.get(id));
}

export async function listSketches(): Promise<SketchMeta[]> {
  const all = await withStore<Sketch[]>("readonly", (s) => s.getAll());
  return all
    .map(({ files: _files, assets: _assets, ...meta }) => meta)
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
  return createSketch(`${sketch.name} (copy)`, sketch.files, sketch.assets, sketch.forkedFrom);
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
