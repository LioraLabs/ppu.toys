import { useEffect, useMemo, useState } from "react";
import { MockPpuCore } from "../ppu/mock";
import type { LuaError } from "../ppu/core";
import { CodeEditor } from "./editor/CodeEditor";
import { useTransportRuntimeError } from "./transport/transport";
import { DEMOS } from "./demos/demos";
import { loadDemo } from "./demos/loadDemo";

export interface EditorPaneProps {
  /** PpuCore.setSource-shaped sink. Defaults to a local mock so the editor is
   *  self-sufficient; Studio passes the shared core's setSource once wired. */
  onSource?: (src: string) => { ok: boolean; error?: LuaError };
}

export function EditorPane({ onSource }: EditorPaneProps) {
  // local fallback core so setSource is genuinely exercised standalone
  const fallback = useMemo(() => new MockPpuCore(), []);
  const sink = onSource ?? ((src: string) => fallback.setSource(src));
  const runtimeError = useTransportRuntimeError();

  const [demoId, setDemoId] = useState(DEMOS[0].id);
  const demo = DEMOS.find((d) => d.id === demoId) ?? DEMOS[0];

  // load the active demo's bundled sources into the core + asset store; re-run
  // on every selection. The keyed CodeEditor below remounts with the new source
  // and re-runs setSource, so the loader only needs to handle assets.
  useEffect(() => {
    loadDemo(demo);
  }, [demo]);

  return (
    <section className="editor">
      <div className="editor-tabs">
        {DEMOS.map((d) => (
          <button
            key={d.id}
            type="button"
            className={"etab" + (d.id === demoId ? " etab--active" : "")}
            onClick={() => setDemoId(d.id)}
          >
            {d.id === demoId && <span className="etab-dot" />}
            {d.label}.lua
          </button>
        ))}
        <div className="etab-spacer" />
        <div className="etab-status">vim · Lua 5.4</div>
      </div>
      <div className="editor-body" data-editor-slot>
        <CodeEditor key={demoId} initialDoc={demo.source} onSource={sink} runtimeError={runtimeError} />
      </div>
    </section>
  );
}
