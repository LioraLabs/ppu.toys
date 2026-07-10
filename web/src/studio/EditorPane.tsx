import { useEffect, useMemo, useRef, useState } from "react";
import type { LuaError, SourceFile } from "../ppu/core";
import { CodeEditor } from "./editor/CodeEditor";
import { FileTabs } from "./editor/FileTabs";
import { routeErrorsByFile } from "./editor/diagnostics";
import { createSourcePusher } from "./editor/sourcePush";
import { transport, useTransportRuntimeError } from "./transport/transport";
import { openSketchStore, useOpenSketch, openContextFiles } from "./sketches/openSketch";
import { restoreOpenContext } from "./sketches/restore";

export interface EditorPaneProps {
  /** PpuCore.setSources-shaped sink for the whole multi-file program in
   *  execution order. Defaults to the shared transport — a bare
   *  `<EditorPane />` is fully wired. */
  onSources?: (files: SourceFile[]) => { ok: boolean; error?: LuaError };
  /** @deprecated Pre-M9 single-file sugar (the old Studio mount passes
   *  transport.setSource). Honored only while the sketch has exactly ONE
   *  file; multi-file programs push through the shared transport. The
   *  Workspace-shell remount should drop this for onSources / the default. */
  onSource?: (src: string) => { ok: boolean; error?: LuaError };
}

export function EditorPane({ onSources, onSource }: EditorPaneProps) {
  const state = useOpenSketch();
  const { session } = state;
  const files = openContextFiles(state);
  const runtimeError = useTransportRuntimeError();
  const [compileError, setCompileError] = useState<LuaError | undefined>(undefined);

  // (re)load the context's assets on every EXPLICIT open. Keyed on session so
  // a lazy fork (same session, same live assets) does not reload anything.
  useEffect(() => {
    let cancelled = false;
    restoreOpenContext(openSketchStore.state().context, () => cancelled).catch((e) =>
      console.error("asset restore failed", e),
    );
    return () => {
      cancelled = true;
    };
  }, [session]);

  // ── active tab: by name, clamped to the live list (deletes/renames)
  const [activeName, setActiveName] = useState(() => files[0]?.name ?? "main.lua");
  const active = files.some((f) => f.name === activeName)
    ? activeName
    : (files[0]?.name ?? "main.lua");
  useEffect(() => {
    setActiveName(openContextFiles(openSketchStore.state())[0]?.name ?? "main.lua");
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
  sinkRef.current = (fs) =>
    onSources
      ? onSources(fs)
      : onSource && fs.length === 1
        ? onSource(fs[0].source)
        : transport.setSources(fs);
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

  // ── route {file,line} errors: inline on the open tab, dots on the rest
  const routed = routeErrorsByFile(
    files.map((f) => f.name),
    active,
    [compileError, runtimeError],
  );
  const errorFiles = new Set(routed.keys());

  const rename = (from: string, to: string): boolean => {
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
    openSketchStore.deleteFile(name);
    docKeys.delete(name); // key is never reused: a re-added name gets a fresh doc
  };

  const activeFile = files.find((f) => f.name === active);

  return (
    <section className="editor">
      <FileTabs
        files={files.map((f) => f.name)}
        active={active}
        errorFiles={errorFiles}
        onSelect={setActiveName}
        onAdd={() => setActiveName(openSketchStore.addFile())}
        onRename={rename}
        onDelete={remove}
        onReorder={(from, to) => openSketchStore.moveFile(from, to)}
      />
      <div className="editor-body" data-editor-slot>
        <CodeEditor
          key={session}
          docKey={keyFor(active)}
          doc={activeFile?.source ?? ""}
          onChange={(src) => openSketchStore.editFile(active, src)}
          errors={routed.get(active) ?? []}
        />
      </div>
    </section>
  );
}
