import { useSyncExternalStore } from "react";
import { DEMOS } from "../demos/demos";
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
  mode = 0
  brightness = 15
  cgram[0] = hsl(230, 0.5, 0.12 + 0.04 * sin(t)) -- breathing backdrop
end
`;

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

function mutateSketch(update: (s: Sketch) => Sketch) {
  const ctx = context;
  if (ctx.kind !== "sketch") return;
  context = { kind: "sketch", sketch: update(ctx.sketch) };
  dirty = true;
  gen++;
  schedule();
  emit();
}

/** Swap the demo context for a brand-new in-memory sketch. Synchronous by
 *  design: no await window in which a second keystroke could double-fork.
 *  Persistence rides the scheduled autosave (saveSketch upserts). */
function forkFromDemo(demoId: string, files: SketchFile[]) {
  const label = DEMOS.find((d) => d.id === demoId)?.label ?? demoId;
  context = { kind: "sketch", sketch: newSketchObject(`${label} (copy)`, files, [], demoId) };
  dirty = true;
  gen++;
  schedule();
  emit();
}

/** Ordered files of a context (demo presents as a single main.lua). */
function filesOf(ctx: OpenContext): SketchFile[] {
  if (ctx.kind === "sketch") return ctx.sketch.files;
  const src = DEMOS.find((d) => d.id === ctx.demoId)?.source ?? "";
  return [{ name: "main.lua", source: src }];
}

/** Files of the LIVE context. */
function currentFiles(): SketchFile[] {
  return filesOf(context);
}

/** Transform the open context's ordered files. Any file operation IS an edit,
 *  so a demo context forks first, carrying its file list through `update`. */
function mutateFiles(update: (files: SketchFile[]) => SketchFile[]) {
  const ctx = context;
  if (ctx.kind === "demo") {
    forkFromDemo(ctx.demoId, update(currentFiles()));
    return;
  }
  mutateSketch((s) => ({ ...s, files: update(s.files) }));
}

function openContext(next: OpenContext) {
  gen++; // invalidate any in-flight flush's state patch (its write still lands)
  context = next;
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
      { name: "main.lua", source: NEW_SKETCH_SOURCE },
    ]);
    openContext({ kind: "sketch", sketch });
  },

  /** The editor doc changed. No-ops when the content is unchanged, so a
   *  pristine write-back can never fork; the first REAL edit of a demo forks it. */
  editFile(name: string, source: string): void {
    const ctx = context;
    if (ctx.kind === "demo") {
      const demoSrc = DEMOS.find((d) => d.id === ctx.demoId)?.source;
      if (demoSrc === source) return; // pristine content, not an edit
      forkFromDemo(ctx.demoId, [{ name, source }]);
      return;
    }
    const existing = ctx.sketch.files.find((f) => f.name === name);
    if (existing && existing.source === source) return;
    mutateSketch((s) => ({
      ...s,
      files: existing
        ? s.files.map((f) => (f.name === name ? { name, source } : f))
        : [...s.files, { name, source }],
    }));
  },

  /** Append a new empty file with a unique fileN.lua name; returns the name.
   *  Order is execution order — new files run last. Demos fork (add IS an edit). */
  addFile(): string {
    const taken = new Set(currentFiles().map((f) => f.name));
    let n = taken.size + 1;
    while (taken.has(`file${n}.lua`)) n++;
    const name = `file${n}.lua`;
    mutateFiles((files) => [...files, { name, source: "" }]);
    return name;
  },

  /** Rename a file. Returns false (and no-ops) on empty/unknown/duplicate
   *  names. Renaming a demo's file forks it. */
  renameFile(from: string, to: string): boolean {
    const next = to.trim();
    const files = currentFiles();
    if (!next || next === from) return false;
    if (!files.some((f) => f.name === from)) return false;
    if (files.some((f) => f.name === next)) return false;
    mutateFiles((fs) => fs.map((f) => (f.name === from ? { ...f, name: next } : f)));
    return true;
  },

  /** Delete a file. Refuses the last one — a sketch always has >= 1 file. */
  deleteFile(name: string): void {
    const files = currentFiles();
    if (files.length <= 1 || !files.some((f) => f.name === name)) return;
    mutateFiles((fs) => fs.filter((f) => f.name !== name));
  },

  /** Move files[from] to index `to`. Order is EXECUTION order (PICO-8). */
  moveFile(from: number, to: number): void {
    const len = currentFiles().length;
    if (from === to || from < 0 || to < 0 || from >= len || to >= len) return;
    mutateFiles((fs) => {
      const next = [...fs];
      const [moved] = next.splice(from, 1);
      next.splice(to, 0, moved);
      return next;
    });
  },

  /** Record an uploaded PNG into the open sketch (an upload IS an edit, so a
   *  demo forks first — with its pristine source, since any prior edit would
   *  already have forked it). Same-named uploads replace. */
  addAsset(asset: SketchAsset): void {
    const ctx = context;
    if (ctx.kind === "demo") {
      const demoSrc = DEMOS.find((d) => d.id === ctx.demoId)?.source ?? "";
      forkFromDemo(ctx.demoId, [{ name: "main.lua", source: demoSrc }]);
    }
    mutateSketch((s) => ({
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

/** Ordered files of the open context — the editor's tab list. A demo presents
 *  as a single read-only main.lua (the first edit forks it). */
export function openContextFiles(s: OpenSketchState): SketchFile[] {
  return filesOf(s.context);
}
