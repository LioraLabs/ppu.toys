import { useMemo } from "react";
import { MockPpuCore } from "../ppu/mock";
import type { LuaError } from "../ppu/core";
import { CodeEditor } from "./editor/CodeEditor";

const SAMPLE = `-- ppu.toys :: dusk-parallax
local SPEED = 12

function frame(t, f)
  mode = 1
  brightness = 15
  -- animated backdrop (cgram[0]) — visible without uploading any assets
  cgram[0] = hsl((t * 40) % 360, 0.6, 0.4)
  -- set bg[1].source = "sky" (drag a PNG onto the dock) to see a scrolling layer
  bg[1].scroll.x = t * SPEED
end`;

export interface EditorPaneProps {
  /** PpuCore.setSource-shaped sink. Defaults to a local mock so the editor is
   *  self-sufficient; Studio can pass the shared core's setSource once wired. */
  onSource?: (src: string) => { ok: boolean; error?: LuaError };
}

export function EditorPane({ onSource }: EditorPaneProps) {
  // local fallback core so setSource is genuinely exercised standalone
  const fallback = useMemo(() => new MockPpuCore(), []);
  const sink = onSource ?? ((src: string) => fallback.setSource(src));

  return (
    <section className="editor">
      <div className="editor-tabs">
        <div className="etab etab--active">
          <span className="etab-dot" />
          main.lua
        </div>
        <div className="etab">mode7.lua</div>
        <div className="etab-spacer" />
        <div className="etab-status">vim · Lua 5.4</div>
      </div>
      <div className="editor-body" data-editor-slot>
        <CodeEditor initialDoc={SAMPLE} onSource={sink} />
      </div>
    </section>
  );
}
