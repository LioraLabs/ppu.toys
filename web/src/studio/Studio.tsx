import { useEffect, useState } from "react";
import type { DockviewApi } from "dockview-react";
import { StudioDock, LayoutMenu } from "./StudioDock";
import { ToolbarWired } from "./ToolbarWired";
import { EditorPane } from "./EditorPane";
import { OutputCanvas } from "./output/OutputCanvas";
import { Inspector } from "./inspector/Inspector";
import { AssetsPanel } from "./sources/AssetsPanel";
import { ErrorBoundary } from "./ErrorBoundary";
import { transport } from "./transport/transport";
import { useOpenSketch, openContextLabel } from "./sketches/openSketch";
import { useDocumentTitle } from "../routes/useDocumentTitle";

/** The wired studio: a toolbar over the dockable shell (StudioDock) — four
 *  panels the user arranges freely: CODE (EditorPane), ASSETS, OUTPUT (the
 *  transport-driven live demo) and INSPECTOR. The composition is available
 *  wasm-free via StudioDock.fixture (fixture-fed slots); this wired shell is
 *  available there too as the opt-in `CoreStage` live fixture. */
export function Studio() {
  const state = useOpenSketch();
  const { dirty } = state;
  const sketchName = openContextLabel(state);
  const [dockApi, setDockApi] = useState<DockviewApi | null>(null);
  useDocumentTitle(`${sketchName} · Studio`);

  // Ctrl/Cmd+Enter = ▶ Run, everywhere in the studio. Capture phase so it wins
  // over CodeMirror's own Enter handling while focus is in the editor.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
        e.preventDefault();
        e.stopPropagation();
        transport.restart();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);
  return (
    <div className="studio">
      <ToolbarWired
        sketchName={sketchName}
        dirty={dirty}
        layoutSlot={dockApi ? <LayoutMenu api={dockApi} /> : null}
      />
      <StudioDock
        editor={<EditorPane onSources={transport.setSources} />}
        assets={<AssetsPanel />}
        output={
          <ErrorBoundary label="Output">
            <OutputCanvas />
          </ErrorBoundary>
        }
        inspector={
          <ErrorBoundary label="Inspector">
            <Inspector />
          </ErrorBoundary>
        }
        onApi={setDockApi}
      />
    </div>
  );
}
