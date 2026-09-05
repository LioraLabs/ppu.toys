import { useSyncExternalStore } from "react";
import { getStarterTemplate } from "../../api/apiClient";
import { POKES_FILE, EMPTY_POKES } from "../pokes/pokes";
import { TIMELINE_FILE, markerSource, DEFAULT_TIMELINE, syncTimeline } from "../output/timeline";

import {
  newSketchObject,
  createSketch,
  saveSketch,
  loadSketch,
  type Sketch,
  type SketchSource,
  type SketchFile,
  type SketchOrigin,
} from "./sketchStore";

export const GENERATED_FILES: ReadonlySet<string> = new Set([POKES_FILE, TIMELINE_FILE]);

/** Debounce window between the last change and the autosave write. */
export const AUTOSAVE_MS = 800;

export const NEW_SKETCH_SOURCE = `function frame(t, f)
  apply_pokes()
  brightness = 15
  cgram[0] = rgb(80 + 60 * math.sin(t), 40, 140)
end
`;

/** The entry file. Not special-cased by the engine (any file can define
 *  frame/init) but it IS the toy's front door — every tutorial and demo
 *  names it — so it can't be deleted or renamed away. */
export const MAIN_FILE = "main.lua";

/** Generated files are always present and pinned before user code. */
function ensureGeneratedFirst(files: SketchFile[]): SketchFile[] {
  const pokes = files.find((f) => f.name === POKES_FILE) ?? {
    name: POKES_FILE,
    source: EMPTY_POKES,
  };
  const timeline = files.find((f) => f.name === TIMELINE_FILE) ?? {
    name: TIMELINE_FILE,
    source: markerSource([], DEFAULT_TIMELINE),
  };
  return [pokes, timeline, ...files.filter((f) => !GENERATED_FILES.has(f.name))];
}

export type OpenContext = { kind: "sketch"; sketch: Sketch };

export interface OpenSketchState {
  context: OpenContext;
  /** Unsaved changes since the last autosave flush. Seam for the toolbar
   *  unsaved dot: `useOpenSketch().dirty`. */
  dirty: boolean;
  /** Bumps only on explicit opens (demo tab, library row, New) — NOT on lazy
   *  fork — so the editor keys its mount on it and survives forking. */
  session: number;
}

const FALLBACK_FILES = [{ name: MAIN_FILE, source: NEW_SKETCH_SOURCE }];

function starterSketch(name = "untitled toy", files: SketchFile[] = FALLBACK_FILES): Sketch {
  return newSketchObject(
    name,
    ensureGeneratedFirst([
      { name: POKES_FILE, source: EMPTY_POKES },
      ...files.filter((file) => file.name !== POKES_FILE),
    ]),
  );
}

let context: OpenContext = { kind: "sketch", sketch: starterSketch() };
let dirty = false;
let session = 0;
/** Mutation counter: lets an in-flight flush detect edits that raced it. */
let gen = 0;
let timer: ReturnType<typeof setTimeout> | null = null;
let snapshot: OpenSketchState = { context, dirty, session };
syncTimeline(context.sketch.files, session);
let starterPromise: ReturnType<typeof getStarterTemplate> | null = null;

function loadStarter() {
  if (!starterPromise) {
    starterPromise = getStarterTemplate().catch(() => {
      starterPromise = null;
      return { name: "untitled toy", files: FALLBACK_FILES };
    });
  }
  return starterPromise;
}

const listeners = new Set<() => void>();
function emit() {
  snapshot = { context, dirty, session };
  syncTimeline(context.sketch.files, session);
  for (const l of listeners) l();
}

function schedule() {
  if (timer) clearTimeout(timer);
  timer = setTimeout(() => {
    // on failure dirty stays true (unsaved dot persists); the next edit retries
    flush().catch((e) => console.error("sketch autosave failed", e));
  }, AUTOSAVE_MS);
}

/** Persist the open sketch now (no-op when clean or on a demo). Captures its
 *  input synchronously, so callers may switch context right after calling. */
async function flush(): Promise<void> {
  if (timer) {
    clearTimeout(timer);
    timer = null;
  }
  const ctx = context;
  if (!dirty) return;
  const flushedGen = gen;
  const saved = await saveSketch(ctx.sketch);
  // a newer edit or a context switch raced the save: leave state alone,
  // the newer timer/flush owns it
  if (gen !== flushedGen) return;
  context = { kind: "sketch", sketch: saved };
  dirty = false;
  emit();
}

function mutateSketch(update: (s: Sketch) => Sketch) {
  context = { kind: "sketch", sketch: update(context.sketch) };
  dirty = true;
  gen++;
  schedule();
  emit();
}

/** The open context as a mutable Sketch: the live sketch, or — for a demo —
 *  a brand-new in-memory fork ("<label> (copy)", pristine files, no sources). */
/** Transform the open context's sketch. Any mutation IS an edit, so a demo
 *  context forks first, with `update` applied to the fresh fork in the SAME
 *  emit. Synchronous by design: no await window in which a second keystroke
 *  could double-fork; `session` is untouched, so the editor survives the
 *  lazy fork. Persistence rides the scheduled autosave (saveSketch upserts). */
function mutateOpen(update: (s: Sketch) => Sketch) {
  const next = update(context.sketch);
  context = { kind: "sketch", sketch: { ...next, files: ensureGeneratedFirst(next.files) } };
  dirty = true;
  gen++;
  schedule();
  emit();
}

/** Ordered files of a context (single-file demos present as one main.lua;
 *  multi-file demos present as their ordered files). A sketch context's
 *  files are already normalized (see openContext/mutateOpen); a demo
 *  context's files are normalized here on read too — demos.ts already ships
 *  pokes.lua first, so this is a no-op reassert, kept as the single seam
 *  that guarantees it regardless of how demos.ts is authored. */
function filesOf(ctx: OpenContext): SketchFile[] {
  return ctx.sketch.files;
}

/** Files of the LIVE context. */
function currentFiles(): SketchFile[] {
  return filesOf(context);
}

/** Transform the open context's ordered files (fork-aware, one emit). */
function mutateFiles(update: (files: SketchFile[]) => SketchFile[]) {
  mutateOpen((s) => ({ ...s, files: update(s.files) }));
}

function openContext(next: OpenContext) {
  gen++; // invalidate any in-flight flush's state patch (its write still lands)
  context = {
    kind: "sketch",
    sketch: { ...next.sketch, files: ensureGeneratedFirst(next.sketch.files) },
  };
  dirty = false;
  session++;
  emit();
}

export const openSketchStore = {
  state: (): OpenSketchState => snapshot,
  subscribe(cb: () => void): () => void {
    listeners.add(cb);
    return () => void listeners.delete(cb);
  },

  /** Replace only the untouched boot fallback with the server-owned starter. */
  async initializeStarter(): Promise<void> {
    const initialGen = gen;
    const starter = await loadStarter();
    if (gen === initialGen && session === 0 && !dirty) {
      context = { kind: "sketch", sketch: starterSketch(starter.name, starter.files) };
      emit();
    }
  },

  /** Open a stored sketch from the library. */
  async openSketch(id: string): Promise<void> {
    await flush();
    const sketch = await loadSketch(id);
    if (!sketch) return;
    openContext({ kind: "sketch", sketch });
  },

  /** Create a sketch from the server-owned starter and open it. */
  async newSketch(): Promise<void> {
    await flush();
    const starter = await loadStarter();
    const sketch = await createSketch(starter.name, [
      { name: POKES_FILE, source: EMPTY_POKES },
      ...starter.files.filter((file) => file.name !== POKES_FILE),
    ]);
    openContext({ kind: "sketch", sketch });
  },

  /** The editor doc changed. No-ops when the content is unchanged, so a
   *  pristine write-back can never fork; the first REAL edit of a demo forks it.
   *  pokes.lua's CRUD reservation (below) is about user-facing add/rename/
   *  delete/reorder — the poke store's own write path calls editFile(POKES_FILE,
   *  ...) directly and must keep working. */
  editFile(name: string, source: string): void {
    const cur = currentFiles().find((f) => f.name === name);
    if (cur && cur.source === source) return; // pristine content, not an edit
    mutateFiles((files) =>
      files.some((f) => f.name === name)
        ? files.map((f) => (f.name === name ? { name, source } : f))
        : [...files, { name, source }],
    );
  },

  /** Append a new empty file with a unique fileN.lua name; returns the name.
   *  Order is execution order — new files run last. Demos fork (add IS an edit).
   *  Generated files don't count toward the numbering and
   *  can never collide with the fileN.lua pattern, but the exclusion is kept
   *  explicit for clarity. */
  addFile(): string {
    const files = currentFiles();
    const taken = new Set(files.map((f) => f.name));
    let n = files.filter((f) => !GENERATED_FILES.has(f.name)).length + 1;
    while (taken.has(`file${n}.lua`)) n++;
    const name = `file${n}.lua`;
    mutateFiles((fs) => [...fs, { name, source: "" }]);
    return name;
  },

  /** Rename a file. Returns false (and no-ops) on empty/unknown/duplicate
   *  names, on touching a generated file (as source or target), or on
   *  renaming main.lua away (that IS a delete). Renaming a demo's file forks it. */
  renameFile(from: string, to: string): boolean {
    const next = to.trim();
    if (GENERATED_FILES.has(from) || GENERATED_FILES.has(next)) return false;
    if (from === MAIN_FILE) return false;
    const files = currentFiles();
    if (!next || next === from) return false;
    if (!files.some((f) => f.name === from)) return false;
    if (files.some((f) => f.name === next)) return false;
    mutateFiles((fs) => fs.map((f) => (f.name === from ? { ...f, name: next } : f)));
    return true;
  },

  /** Delete a file. No-ops on generated files and on main.lua. Refuses
   *  the last user file — a sketch always has >= 1 user file. */
  deleteFile(name: string): void {
    if (GENERATED_FILES.has(name) || name === MAIN_FILE) return;
    const files = currentFiles();
    const realCount = files.filter((f) => !GENERATED_FILES.has(f.name)).length;
    if (realCount <= 1 || !files.some((f) => f.name === name)) return;
    mutateFiles((fs) => fs.filter((f) => f.name !== name));
  },

  /** Move files[from] to index `to`. Order is EXECUTION order (PICO-8).
   *  No-ops if either endpoint is a pinned generated file. */
  moveFile(from: number, to: number): void {
    const len = currentFiles().length;
    if (from === to || from < 0 || to < 0 || from >= len || to >= len) return;
    if (from < GENERATED_FILES.size || to < GENERATED_FILES.size) return;
    mutateFiles((fs) => {
      const next = [...fs];
      const [moved] = next.splice(from, 1);
      next.splice(to, 0, moved);
      return next;
    });
  },

  /** Record a converted graphics source into the open sketch (an add IS an edit,
   *  so a demo forks first). Same-named sources replace. One emit. */
  addSource(source: SketchSource): void {
    mutateOpen((s) => ({
      ...s,
      sources: s.sources.some((x) => x.name === source.name)
        ? s.sources.map((x) => (x.name === source.name ? source : x))
        : [...s.sources, source],
    }));
  },

  /** Drop a recorded source from the open sketch (a remove IS an edit, so a
   *  demo forks first). Returns whether the name was present. The caller owns
   *  the engine side (transport.removeSource) — same split as addSource. */
  removeSource(name: string): boolean {
    let found = false;
    mutateOpen((s) => {
      found = s.sources.some((x) => x.name === name);
      return found ? { ...s, sources: s.sources.filter((x) => x.name !== name) } : s;
    });
    return found;
  },

  /** Rename the OPEN sketch through the live context (renaming it directly in
   *  the store would be reverted by the next autosave flush). */
  rename(name: string): void {
    mutateSketch((s) => ({ ...s, name }));
  },

  /** Link the open sketch to a published toy. Rides the autosave like any edit. */
  setOrigin(origin: SketchOrigin): void {
    mutateSketch((s) => ({ ...s, origin }));
  },

  /** Unlink the open sketch. Drops the key rather than setting it undefined. */
  clearOrigin(): void {
    mutateSketch(({ origin: _origin, ...s }) => s);
  },

  /** Persist pending changes now (autosave uses this; tests + open paths too). */
  flush,

  /** Test hook: back to the boot state. */
  _resetForTests(): void {
    if (timer) {
      clearTimeout(timer);
      timer = null;
    }
    context = { kind: "sketch", sketch: starterSketch() };
    dirty = false;
    session = 0;
    gen++;
    starterPromise = null;
    emit();
  },
};

export function useOpenSketch(): OpenSketchState {
  return useSyncExternalStore(openSketchStore.subscribe, openSketchStore.state);
}

/** Display name of the open context — the toolbar seam for the Workspace shell. */
export function openContextLabel(s: OpenSketchState): string {
  return s.context.sketch.name;
}

/** Ordered files of the open context — the editor's tab list. A single-file
 *  demo presents as one read-only main.lua; a multi-file demo presents as its
 *  ordered files (the first edit to any of them forks it). */
export function openContextFiles(s: OpenSketchState): SketchFile[] {
  return filesOf(s.context);
}
