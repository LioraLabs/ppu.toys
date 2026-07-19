import { useState } from "react";
import type { ReactNode } from "react";
import type { FrameResult, SourceFile } from "../ppu/core";
import { WIDTH, HEIGHT } from "../ppu/core";
import { StudioLayout } from "./StudioLayout";
import { Studio } from "./Studio";
import { Toolbar } from "./Toolbar";
import { ActivityRail } from "./ActivityRail";
import { FileTabs } from "./editor/FileTabs";
import { CodeEditor } from "./editor/CodeEditor";
import { DropZone } from "./output/DropZone";
import { Inspector } from "./inspector/Inspector";
import { InspectorFrameProvider } from "./inspector/useInspectorFrame";
import type { OverlayId, TabId } from "./inspector/tabs";
import { ModeBadge, PlaneSeg, TraceCaption } from "./inspector/tracemem/TraceChain";
import { MemoryTab } from "./inspector/MemoryTab";
import { ComposeTab } from "./inspector/ComposeTab";
import { WindowsTab } from "./inspector/WindowsTab";
import { RegistersTab } from "./inspector/RegistersTab";
import { SpritesTab } from "./inspector/SpritesTab";
import { VramTab } from "./inspector/VramTab";
import { MemoryLayersOverlay } from "./inspector/MemoryLayersOverlay";
import { CompositorOverlay } from "./inspector/CompositorOverlay";
import { BlitCanvas } from "./inspector/BlitCanvas";
import { makeFixtureCompositor } from "./inspector/compose/storyCompositor";
import {
  sketchName,
  sketchFiles,
  makeFrameResult,
  frameVram,
  frameImportReports,
  frameScreens,
} from "../fixtures";
import { CoreStage } from "../cosmos/FixtureStage";
import "./inspector/tracemem/tracemem.css";
import "./inspector/compose/compose.css";
import "./pokes/pokes.css";

// The composed studio shell — the level ABOVE the ~88 leaf stories. `Composed`
// assembles the real layout (StudioLayout), the real chrome (Toolbar,
// ActivityRail), the real editor leaves (FileTabs + CodeEditor) and the real
// Inspector chrome entirely from fixtures, so the whole-app arrangement
// (grid, columns, panel adjacency, theming) is editable wasm-free with the
// usual fixture/shoot loop. Only the truly rasterizer-bound surfaces are
// substituted: the live WebGL output becomes a BlitCanvas of the fixture
// framebuffer, and the trace resolution chain becomes a pointer to LiveCore.
// ── fixture frame with visible pixels ────────────────────────────────────────
// makeFrameResult's framebuffer is zeroed (black); paint a deterministic
// synthetic scene — sky gradient over a ground band — so the output panel and
// layout read like a running sketch without any rasterizer.
function syntheticFramebuffer(): Uint8ClampedArray {
  const px = new Uint8ClampedArray(WIDTH * HEIGHT * 4);
  const horizon = Math.floor(HEIGHT * 0.72);
  for (let y = 0; y < HEIGHT; y++) {
    for (let x = 0; x < WIDTH; x++) {
      const i = (y * WIDTH + x) * 4;
      if (y < horizon) {
        const t = y / horizon; // deep blue → dusk magenta
        px[i] = 40 + t * 140;
        px[i + 1] = 30 + t * 40;
        px[i + 2] = 90 + t * 70;
      } else {
        const t = (y - horizon) / (HEIGHT - horizon); // dark teal ground
        px[i] = 20;
        px[i + 1] = 60 + t * 50;
        px[i + 2] = 70 + t * 30;
      }
      px[i + 3] = 255;
    }
  }
  return px;
}

const frame: FrameResult = makeFrameResult({ framebuffer: syntheticFramebuffer() });
const compositor = makeFixtureCompositor(frame);

// ── fixture inspector slots ──────────────────────────────────────────────────
function CoreNote({ what }: { what: string }) {
  return (
    <div className="tm-note">
      {what} is produced by the live rasterizer (ppuCore) and can't render from
      a static fixture — open the LiveCore story for the real thing.
    </div>
  );
}

function fixtureTab(tab: TabId, f: FrameResult): ReactNode {
  switch (tab) {
    case "trace":
      return (
        <div className="insp-scroll">
          <div className="tm-controls">
            <PlaneSeg />
            <ModeBadge frame={f} />
          </div>
          <TraceCaption frame={f} />
          <CoreNote what="The Stage 1–5 resolution chain" />
        </div>
      );
    case "memory":
      return <MemoryTab frame={f} vram={frameVram} />;
    case "compose":
      return <ComposeTab c={compositor} screens={frameScreens} />;
    case "windows":
      // WindowsTab's compositor sits on injectable seams (inspector frame +
      // poke store), so the real tab renders wasm-free under the provider.
      return <WindowsTab />;
    case "registers":
      return <RegistersTab frame={f} />;
    case "sprites":
      return <SpritesTab frame={f} />;
    case "vram":
      return <VramTab frame={f} vram={frameVram} reports={frameImportReports} />;
  }
}

function fixtureOverlay(overlay: OverlayId, f: FrameResult, onCollapse: () => void): ReactNode {
  return overlay === "memory-layers" ? (
    <MemoryLayersOverlay
      onCollapse={onCollapse}
      frame={f}
      vram={frameVram}
      reports={frameImportReports}
      chain={() => <CoreNote what="The resolution chain" />}
    />
  ) : (
    <CompositorOverlay onCollapse={onCollapse} c={compositor} screens={frameScreens} />
  );
}

// ── editor slot: real FileTabs + CodeEditor over story-local file state ──────
const GENERATED: ReadonlySet<string> = new Set(["pokes.lua"]);
const NO_ERRORS: ReadonlySet<string> = new Set();

function EditorMock() {
  const [files, setFiles] = useState<SourceFile[]>(() => sketchFiles.map((f) => ({ ...f })));
  const [active, setActive] = useState("main.lua");
  const activeFile = files.find((f) => f.name === active);
  const rename = (from: string, to: string): boolean => {
    if (!to || files.some((f) => f.name === to)) return false;
    setFiles((fs) => fs.map((f) => (f.name === from ? { ...f, name: to } : f)));
    if (active === from) setActive(to);
    return true;
  };
  return (
    <section className="editor">
      <FileTabs
        files={files.map((f) => f.name)}
        active={active}
        errorFiles={NO_ERRORS}
        generated={GENERATED}
        onSelect={setActive}
        onAdd={() => {
          const name = `untitled${files.length}.lua`;
          setFiles((fs) => [...fs, { name, source: "-- new file\n" }]);
          setActive(name);
        }}
        onRename={rename}
        onDelete={(name) => {
          setFiles((fs) => fs.filter((f) => f.name !== name));
          if (active === name) setActive("main.lua");
        }}
        onReorder={(from, to) =>
          setFiles((fs) => {
            const next = [...fs];
            next.splice(to, 0, ...next.splice(from, 1));
            return next;
          })
        }
      />
      <div className="editor-body" data-editor-slot>
        <CodeEditor
          docKey={active}
          doc={activeFile?.source ?? ""}
          generated={GENERATED.has(active)}
          onChange={(src) =>
            setFiles((fs) => fs.map((f) => (f.name === active ? { ...f, source: src } : f)))
          }
          errors={[]}
        />
      </div>
    </section>
  );
}

// ── output slot: OutputCanvas's markup with the fixture framebuffer blitted
//    where the WebGL present pass would be, and inert transport chrome ────────
function OutputMock({ f }: { f: FrameResult }) {
  return (
    <div className="output">
      <div className="output-header">
        <span className="output-title">LIVE OUTPUT</span>
        <div className="tb-spacer" />
        <button type="button" className="fx-toggle">CRT</button>
        <button type="button" className="fx-toggle">SCAN</button>
        <button type="button" className="fx-toggle">GRID</button>
        <span className="pill">MODE 1</span>
        <span className="pill">256×224</span>
      </div>
      <div className="output-row">
        <div className="display">
          <BlitCanvas
            className="display-canvas"
            pixels={f.framebuffer}
            width={WIDTH}
            height={HEIGHT}
            title="fixture framebuffer"
          />
          <span className="display-badge">fixture · no core</span>
        </div>
        <div className="output-side">
          <div className="transport">
            <button className="play-btn" aria-label="Play">▶</button>
            <div className="scrubber">
              <div className="scrubber-fill" style={{ width: "35%" }} />
              <div className="scrubber-handle" style={{ left: "35%" }} />
            </div>
          </div>
          <div className="readout">
            <span>t=2.1s</span>
            <span>frame 126</span>
            <span>60fps</span>
          </div>
          <DropZone error={null} onFiles={() => {}} />
        </div>
      </div>
    </div>
  );
}

const toolbarSourceSlot = (
  <button type="button" className="btn-ghost">
    + Source
  </button>
);
const toolbarWorkspaceSlot = (
  <button type="button" className="btn-ghost">
    Save
  </button>
);

function ComposedShell() {
  return (
    <div style={{ position: "relative" }}>
      <StudioLayout
        toolbar={
          <Toolbar
            sketchName={sketchName}
            dirty
            theme="dark"
            sourceSlot={toolbarSourceSlot}
            workspaceSlot={toolbarWorkspaceSlot}
          />
        }
        rail={<ActivityRail active="layers" />}
        editor={<EditorMock />}
        right={
          <aside className="right">
            <OutputMock f={frame} />
            <Inspector renderTab={fixtureTab} renderOverlay={fixtureOverlay} />
          </aside>
        }
      />
    </div>
  );
}

const Composed = () => (
  <InspectorFrameProvider frame={frame}>
    <ComposedShell />
  </InspectorFrameProvider>
);

// The REAL wired studio — wasm core booted by the decorator, network via the
// global MSW worker. This is a living end-to-end demo, not a fixture surface:
// it runs the shared transport's rAF loop and persists sketch/poke state the
// same way the app does.
const LiveCore = () => (
  <CoreStage>
    <Studio />
  </CoreStage>
);
LiveCore.storyName = "Live core";

export default {
  Composed,
  LiveCore,
};
