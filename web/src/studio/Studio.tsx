import { useEffect } from "react";
import { StudioLayout } from "./StudioLayout";
import { ToolbarWired } from "./ToolbarWired";
import { ActivityRailWired } from "./ActivityRailWired";
import { EditorPane } from "./EditorPane";
import { RightColumn } from "./RightColumn";
import { Inspector } from "./inspector/Inspector";
import { inspectorStore, useInspectorView } from "./inspector/inspectorStore";
import { ErrorBoundary } from "./ErrorBoundary";
import { transport } from "./transport/transport";
import { useOpenSketch, openContextLabel } from "./sketches/openSketch";
import { useDocumentTitle } from "../routes/useDocumentTitle";

/** The wired studio: fills StudioLayout's slots with the full app — the sketch
 *  store (openSketch), the CodeMirror EditorPane, the transport-driven
 *  RightColumn (OutputCanvas owns the rAF/wasm loop), and the wired chrome
 *  (ToolbarWired/ActivityRailWired). The composition itself is available
 *  wasm-free via StudioLayout.fixture (fixture-fed slots); this wired shell is
 *  available there too as the opt-in `CoreStage` live fixture. */
export function Studio() {
  const state = useOpenSketch();
  const view = useInspectorView();
  const { dirty } = state;
  const sketchName = openContextLabel(state);
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
    <StudioLayout
      toolbar={<ToolbarWired sketchName={sketchName} dirty={dirty} />}
      rail={<ActivityRailWired />}
      editor={<EditorPane onSources={transport.setSources} />}
      dock={
        <ErrorBoundary label="Inspector">
          <Inspector />
        </ErrorBoundary>
      }
      dockOpen={view.dockOpen}
      onDockToggle={inspectorStore.toggleDock}
      right={<RightColumn />}
    />
  );
}
