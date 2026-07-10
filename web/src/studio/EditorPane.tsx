import { useEffect, useMemo } from "react";
import { MockPpuCore } from "../ppu/mock";
import type { LuaError } from "../ppu/core";
import { CodeEditor } from "./editor/CodeEditor";
import { useTransportRuntimeError } from "./transport/transport";
import { DEMOS } from "./demos/demos";
import { openSketchStore, useOpenSketch } from "./sketches/openSketch";
import { restoreOpenContext } from "./sketches/restore";

export interface EditorPaneProps {
  /** PpuCore.setSource-shaped sink. Defaults to a local mock so the editor is
   *  self-sufficient; Studio passes the shared core's setSource once wired. */
  onSource?: (src: string) => { ok: boolean; error?: LuaError };
}

export function EditorPane({ onSource }: EditorPaneProps) {
  // local fallback core so setSource is genuinely exercised standalone
  const fallback = useMemo(() => new MockPpuCore(), []);
  const coreSink = onSource ?? ((src: string) => fallback.setSource(src));
  const runtimeError = useTransportRuntimeError();

  const { context, session, dirty } = useOpenSketch();

  // (re)load the context's assets on every EXPLICIT open. Keyed on session so
  // a lazy fork (same session, same live assets) does not reload anything.
  // The cleanup cancels a superseded run (StrictMode double-effects, rapid
  // opens) so overlapping restores can't interleave duplicate assets.
  useEffect(() => {
    let cancelled = false;
    restoreOpenContext(openSketchStore.state().context, () => cancelled).catch((e) =>
      console.error("asset restore failed", e),
    );
    return () => {
      cancelled = true;
    };
  }, [session]);

  const fileName =
    context.kind === "sketch" ? context.sketch.files[0]?.name ?? "main.lua" : "main.lua";
  const doc =
    context.kind === "sketch"
      ? context.sketch.files[0]?.source ?? ""
      : DEMOS.find((d) => d.id === context.demoId)?.source ?? "";

  // run the source AND record it into the open sketch; the first change to a
  // demo lazily forks it (openSketch no-ops on the pristine mount push)
  const sink = (src: string) => {
    openSketchStore.editFile(fileName, src);
    return coreSink(src);
  };

  return (
    <section className="editor">
      <div className="editor-tabs">
        {context.kind === "sketch" && (
          <button type="button" className="etab etab--active">
            <span className="etab-dot" />
            {context.sketch.name}
            {dirty ? " *" : ""}
          </button>
        )}
        {DEMOS.map((d) => (
          <button
            key={d.id}
            type="button"
            className={
              "etab" + (context.kind === "demo" && d.id === context.demoId ? " etab--active" : "")
            }
            onClick={() =>
              openSketchStore.openDemo(d.id).catch((e) => console.error("open demo failed", e))
            }
          >
            {context.kind === "demo" && d.id === context.demoId && <span className="etab-dot" />}
            {d.label}.lua
          </button>
        ))}
        <div className="etab-spacer" />
        <div className="etab-status">vim · Lua 5.4</div>
      </div>
      <div className="editor-body" data-editor-slot>
        <CodeEditor key={session} initialDoc={doc} onSource={sink} runtimeError={runtimeError} />
      </div>
    </section>
  );
}
