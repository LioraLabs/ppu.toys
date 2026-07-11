import { useEffect, useMemo, useRef, useState } from "react";
import type { LuaError, SourceFile } from "../ppu/core";
import { CodeEditor } from "./editor/CodeEditor";
import { FileTabs } from "./editor/FileTabs";
import { routeErrorsByFile } from "./editor/diagnostics";
import { createSourcePusher } from "./editor/sourcePush";
import { transport, useTransportRuntimeError } from "./transport/transport";
import { openSketchStore, useOpenSketch, openContextFiles } from "./sketches/openSketch";
import { restoreOpenContext } from "./sketches/restore";
import { POKES_FILE } from "./pokes/pokes";
import { usePokes } from "./pokes/pokeStore";
import { DialectToggle, PokeBar } from "./inspector/compose/chrome";

/** The only machine-generated file — read-only tab, CRUD-guarded (see
 *  openSketchStore), never a default active-tab target. */
const GENERATED = new Set([POKES_FILE]);

/** Stable empty-set identity: reused whenever there are no pokes, so
 *  FileTabs doesn't see a new Set on every render. */
const EMPTY_SET: ReadonlySet<string> = new Set();

/** First non-generated file, falling back to files[0] when every file is
 *  generated (should not happen — a sketch always keeps >= 1 real file). */
function defaultActive(files: SourceFile[]): string {
  return (files.find((f) => !GENERATED.has(f.name)) ?? files[0])?.name ?? "main.lua";
}

export interface EditorPaneProps {
  /** PpuCore.setSources-shaped sink for the whole multi-file program in
   *  execution order. Defaults to the shared transport — a bare
   *  `<EditorPane />` is fully wired. */
  onSources?: (files: SourceFile[]) => { ok: boolean; error?: LuaError };
}

/** Stable empty-errors identity so a clean doc never re-dispatches diagnostics. */
const NO_ERRORS: LuaError[] = [];

/** Poke menu bar shown above the editor body, only while the generated
 *  pokes.lua tab is active — the dialect choice and poke summary are
 *  meaningless context for any other file. */
export function PokeFileBar({ active }: { active: string }) {
  if (active !== POKES_FILE) return null;
  return (
    <div className="poke-filebar">
      <DialectToggle />
      <PokeBar />
    </div>
  );
}

export function EditorPane({ onSources }: EditorPaneProps) {
  const state = useOpenSketch();
  const { session } = state;
  const files = openContextFiles(state);
  const runtimeError = useTransportRuntimeError();
  const [compileError, setCompileError] = useState<LuaError | undefined>(undefined);

  // (re)load the context's assets on every EXPLICIT open. Keyed on session so
  // a lazy fork (same session, same live assets) does not reload anything.
  useEffect(() => {
    let cancelled = false;
    restoreOpenContext(openSketchStore.state().context, () => cancelled);
    return () => {
      cancelled = true;
    };
  }, [session]);

  // ── active tab: by name, clamped to the live list (deletes/renames).
  // Defaults to the first NON-generated file — pokes.lua is always index 0,
  // so without this every sketch would open staring at the generated tab.
  const [activeName, setActiveName] = useState(() => defaultActive(files));
  const active = files.some((f) => f.name === activeName)
    ? activeName
    : defaultActive(files);
  useEffect(() => {
    setActiveName(defaultActive(openContextFiles(openSketchStore.state())));
  }, [session]);

  // ── stable doc identities: survive renames (undo history follows the file),
  //    never reused after a delete. Fresh map per session (editor remounts).
  const docKeys = useMemo(() => new Map<string, string>(), [session]);
  const uid = useRef(0);
  const keyFor = (name: string): string => {
    let k = docKeys.get(name);
    if (!k) {
      k = `doc${uid.current++}`;
      docKeys.set(name, k);
    }
    return k;
  };

  // ── debounced whole-program push (error grace lives in the engine)
  const sinkRef = useRef<(fs: SourceFile[]) => { ok: boolean; error?: LuaError }>(
    () => ({ ok: true }),
  );
  sinkRef.current = (fs) => (onSources ? onSources(fs) : transport.setSources(fs));
  const pusher = useMemo(
    () => createSourcePusher((fs) => sinkRef.current(fs), setCompileError),
    [],
  );
  useEffect(() => () => pusher.dispose(), [pusher]);
  // explicit open: run the program immediately
  useEffect(() => {
    pusher.pushNow(openContextFiles(openSketchStore.state()));
  }, [session, pusher]);
  // any store mutation (edit/add/rename/delete/reorder): debounced re-push.
  // The pusher content-dedupes, so no-op emits (autosave flush) don't recompile.
  useEffect(() => {
    pusher.push(files);
    // files is context-derived; context identity changes on every store emit
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [state.context, pusher]);

  // ── route {file,line} errors: inline on the open tab, dots on the rest.
  // Memoized so the active tab's error array keeps its identity across
  // renders — CodeEditor's diagnostics effect only re-dispatches when the
  // errors (or the doc) actually change, not on every keystroke re-render.
  const routed = useMemo(
    () =>
      routeErrorsByFile(
        files.map((f) => f.name),
        active,
        [compileError, runtimeError],
      ),
    // files derives from context; context identity changes on every store emit
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [state.context, active, compileError, runtimeError],
  );
  const errorFiles = useMemo(() => new Set(routed.keys()), [routed]);

  const rename = (from: string, to: string): boolean => {
    // belt-and-braces: the store already rejects touching the reserved
    // generated file, and FileTabs never wires up its dblclick for it.
    if (GENERATED.has(from) || GENERATED.has(to)) return false;
    if (!openSketchStore.renameFile(from, to)) return false;
    const k = docKeys.get(from);
    if (k !== undefined) {
      docKeys.set(to, k);
      docKeys.delete(from);
    }
    if (activeName === from) setActiveName(to);
    return true;
  };
  const remove = (name: string) => {
    if (GENERATED.has(name)) return; // belt-and-braces, see rename above
    openSketchStore.deleteFile(name);
    docKeys.delete(name); // key is never reused: a re-added name gets a fresh doc
    // re-anchor the NAMED active state too — a later rename to the deleted
    // name must not silently steal activation
    if (activeName === name)
      setActiveName(defaultActive(openContextFiles(openSketchStore.state())));
  };

  const activeFile = files.find((f) => f.name === active);
  const pokedFiles = usePokes().length > 0 ? GENERATED : EMPTY_SET;

  return (
    <section className="editor">
      <FileTabs
        files={files.map((f) => f.name)}
        active={active}
        errorFiles={errorFiles}
        generated={GENERATED}
        pokedFiles={pokedFiles}
        onSelect={setActiveName}
        onAdd={() => setActiveName(openSketchStore.addFile())}
        onRename={rename}
        onDelete={remove}
        onReorder={(from, to) => openSketchStore.moveFile(from, to)}
      />
      <PokeFileBar active={active} />
      <div className="editor-body" data-editor-slot>
        <CodeEditor
          key={session}
          docKey={keyFor(active)}
          doc={activeFile?.source ?? ""}
          generated={GENERATED.has(active)}
          onChange={(src) => openSketchStore.editFile(active, src)}
          errors={routed.get(active) ?? NO_ERRORS}
        />
      </div>
    </section>
  );
}
