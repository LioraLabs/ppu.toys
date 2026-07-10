import { useSyncExternalStore } from "react";
import { DEMOS, demoFiles } from "../demos/demos";
import { POKES_FILE, EMPTY_POKES } from "../pokes/pokes";
import {
  newSketchObject,
  createSketch,
  saveSketch,
  loadSketch,
  type Sketch,
  type SketchAsset,
  type SketchFile,
} from "./sketchStore";

/** Debounce window between the last change and the autosave write. */
export const AUTOSAVE_MS = 800;

export const NEW_SKETCH_SOURCE = `-- ppu.toys sketch — flat SNES PPU globals, Lua 5.4
-- registers: mode, brightness, bg[1..4], cgram[], obj[], m7, vram[]
-- helpers: rgb(r,g,b), hsl(h,s,l), hdma(y0,y1,fn), sin/cos/floor, t, f
function frame(t, f)
  apply_pokes()
  mode = 0
  brightness = 15
  cgram[0] = hsl(230, 0.5, 0.12 + 0.04 * sin(t)) -- breathing backdrop
end
`;

/** pokes.lua is reserved: always present, always index 0. The ONLY generated
 *  file — every point where files enter or are read from the open context
 *  normalizes through this. */
function ensurePokesFirst(files: SketchFile[]): SketchFile[] {
  const pokes = files.find((f) => f.name === POKES_FILE) ?? { name: POKES_FILE, source: EMPTY_POKES };
  return [pokes, ...files.filter((f) => f.name !== POKES_FILE)];
}

/** What the editor is looking at: a read-only bundled demo, or a stored
 *  sketch. Demos become sketches lazily — see editFile/addAsset. */
export type OpenContext =
  | { kind: "demo"; demoId: string }
  | { kind: "sketch"; sketch: Sketch };

export interface OpenSketchState {
  context: OpenContext;
  /** Unsaved changes since the last autosave flush. Seam for the toolbar
   *  unsaved dot: `useOpenSketch().dirty`. */
  dirty: boolean;
  /** Bumps only on explicit opens (demo tab, library row, New) — NOT on lazy
   *  fork — so the editor keys its mount on it and survives forking. */
  session: number;
}

let context: OpenContext = { kind: "demo", demoId: DEMOS[0].id };
let dirty = false;
let session = 0;
/** Mutation counter: lets an in-flight flush detect edits that raced it. */
let gen = 0;
let timer: ReturnType<typeof setTimeout> | null = null;
let snapshot: OpenSketchState = { context, dirty, session };

const listeners = new Set<() => void>();
function emit() {
  snapshot = { context, dirty, session };
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
  if (!dirty || ctx.kind !== "sketch") return;
  const flushedGen = gen;
  const saved = await saveSketch(ctx.sketch);
  // a newer edit or a context switch raced the save: leave state alone,
  // the newer timer/flush owns it
  if (gen !== flushedGen) return;
  context = { kind: "sketch", sketch: saved };
  dirty = false;
  emit();
}

/** Mutate the open SKETCH only — a demo context is a no-op (rename relies
 *  on this; every other mutation goes through the fork-aware mutateOpen). */
function mutateSketch(update: (s: Sketch) => Sketch) {
  const ctx = context;
  if (ctx.kind !== "sketch") return;
  context = { kind: "sketch", sketch: update(ctx.sketch) };
  dirty = true;
  gen++;
  schedule();
  emit();
}

/** The open context as a mutable Sketch: the live sketch, or — for a demo —
 *  a brand-new in-memory fork ("<label> (copy)", pristine files, no assets). */
function sketchToMutate(ctx: OpenContext): Sketch {
  if (ctx.kind === "sketch") return ctx.sketch;
  const label = DEMOS.find((d) => d.id === ctx.demoId)?.label ?? ctx.demoId;
  return newSketchObject(`${label} (copy)`, filesOf(ctx), [], ctx.demoId);
}

/** Transform the open context's sketch. Any mutation IS an edit, so a demo
 *  context forks first, with `update` applied to the fresh fork in the SAME
 *  emit. Synchronous by design: no await window in which a second keystroke
 *  could double-fork; `session` is untouched, so the editor survives the
 *  lazy fork. Persistence rides the scheduled autosave (saveSketch upserts). */
function mutateOpen(update: (s: Sketch) => Sketch) {
  const next = update(sketchToMutate(context));
  context = { kind: "sketch", sketch: { ...next, files: ensurePokesFirst(next.files) } };
  dirty = true;
  gen++;
  schedule();
  emit();
}

/** Ordered files of a context (single-file demos present as one main.lua;
 *  multi-file demos present as their ordered files). A sketch context's
 *  files are already normalized (see openContext/mutateOpen); a demo
 *  context's files are normalized here on read, since demos.ts doesn't
 *  carry pokes.lua yet (Task 10 will bake it in — this is the bridge). */
function filesOf(ctx: OpenContext): SketchFile[] {
  if (ctx.kind === "sketch") return ctx.sketch.files;
  const demo = DEMOS.find((d) => d.id === ctx.demoId);
  return ensurePokesFirst(demo ? demoFiles(demo) : [{ name: "main.lua", source: "" }]);
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
  context =
    next.kind === "sketch"
      ? { kind: "sketch", sketch: { ...next.sketch, files: ensurePokesFirst(next.sketch.files) } }
      : next;
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

  /** Open a bundled demo as a read-only template. Pending edits on the
   *  previous sketch are flushed (captured synchronously, saved async). */
  openDemo(demoId: string): Promise<void> {
    const pending = flush();
    openContext({ kind: "demo", demoId });
    return pending;
  },

  /** Open a stored sketch from the library. */
  async openSketch(id: string): Promise<void> {
    await flush();
    const sketch = await loadSketch(id);
    if (!sketch) return;
    openContext({ kind: "sketch", sketch });
  },

  /** Create a blank sketch and open it. */
  async newSketch(): Promise<void> {
    await flush();
    const sketch = await createSketch("untitled", [
      { name: POKES_FILE, source: EMPTY_POKES },
      { name: "main.lua", source: NEW_SKETCH_SOURCE },
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
   *  pokes.lua doesn't count toward the numbering (it's not a user file) and
   *  can never collide with the fileN.lua pattern, but the exclusion is kept
   *  explicit for clarity. */
  addFile(): string {
    const files = currentFiles();
    const taken = new Set(files.map((f) => f.name));
    let n = files.filter((f) => f.name !== POKES_FILE).length + 1;
    while (taken.has(`file${n}.lua`)) n++;
    const name = `file${n}.lua`;
    mutateFiles((fs) => [...fs, { name, source: "" }]);
    return name;
  },

  /** Rename a file. Returns false (and no-ops) on empty/unknown/duplicate
   *  names, or on touching the reserved pokes.lua (as source or target).
   *  Renaming a demo's file forks it. */
  renameFile(from: string, to: string): boolean {
    const next = to.trim();
    if (from === POKES_FILE || next === POKES_FILE) return false;
    const files = currentFiles();
    if (!next || next === from) return false;
    if (!files.some((f) => f.name === from)) return false;
    if (files.some((f) => f.name === next)) return false;
    mutateFiles((fs) => fs.map((f) => (f.name === from ? { ...f, name: next } : f)));
    return true;
  },

  /** Delete a file. No-ops on the reserved pokes.lua. Refuses the last
   *  REAL (non-pokes) file — a sketch always has >= 1 user file. */
  deleteFile(name: string): void {
    if (name === POKES_FILE) return;
    const files = currentFiles();
    const realCount = files.filter((f) => f.name !== POKES_FILE).length;
    if (realCount <= 1 || !files.some((f) => f.name === name)) return;
    mutateFiles((fs) => fs.filter((f) => f.name !== name));
  },

  /** Move files[from] to index `to`. Order is EXECUTION order (PICO-8).
   *  No-ops if either endpoint is index 0 — pokes.lua is pinned first. */
  moveFile(from: number, to: number): void {
    const len = currentFiles().length;
    if (from === to || from < 0 || to < 0 || from >= len || to >= len) return;
    if (from === 0 || to === 0) return;
    mutateFiles((fs) => {
      const next = [...fs];
      const [moved] = next.splice(from, 1);
      next.splice(to, 0, moved);
      return next;
    });
  },

  /** Record an uploaded PNG into the open sketch (an upload IS an edit, so a
   *  demo forks first — with all its pristine files, since any prior edit would
   *  already have forked it). Same-named uploads replace. One emit. */
  addAsset(asset: SketchAsset): void {
    mutateOpen((s) => ({
      ...s,
      assets: s.assets.some((a) => a.name === asset.name)
        ? s.assets.map((a) => (a.name === asset.name ? asset : a))
        : [...s.assets, asset],
    }));
  },

  /** Rename the OPEN sketch through the live context (renaming it directly in
   *  the store would be reverted by the next autosave flush, which puts the
   *  stale in-memory name back). No-op on a demo context. */
  rename(name: string): void {
    mutateSketch((s) => ({ ...s, name }));
  },

  /** Persist pending changes now (autosave uses this; tests + open paths too). */
  flush,

  /** Test hook: back to the boot state. */
  _resetForTests(): void {
    if (timer) {
      clearTimeout(timer);
      timer = null;
    }
    context = { kind: "demo", demoId: DEMOS[0].id };
    dirty = false;
    session = 0;
    gen++;
    emit();
  },
};

export function useOpenSketch(): OpenSketchState {
  return useSyncExternalStore(openSketchStore.subscribe, openSketchStore.state);
}

/** Display name of the open context — the toolbar seam for the Workspace shell. */
export function openContextLabel(s: OpenSketchState): string {
  const ctx = s.context;
  return ctx.kind === "sketch"
    ? ctx.sketch.name
    : DEMOS.find((d) => d.id === ctx.demoId)?.label ?? ctx.demoId;
}

/** Ordered files of the open context — the editor's tab list. A single-file
 *  demo presents as one read-only main.lua; a multi-file demo presents as its
 *  ordered files (the first edit to any of them forks it). */
export function openContextFiles(s: OpenSketchState): SketchFile[] {
  return filesOf(s.context);
}
