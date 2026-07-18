import "../styles/tokens.css";
import "./studio.css";
import { ToolbarWired } from "./ToolbarWired";
import { ActivityRailWired } from "./ActivityRailWired";
import { EditorPane } from "./EditorPane";
import { RightColumn } from "./RightColumn";
import { transport } from "./transport/transport";
import { useOpenSketch, openContextLabel } from "./sketches/openSketch";

/** The studio shell. Intentionally left wired (no Ladle story): it composes the
 *  full app — the sketch store (openSketch), the CodeMirror EditorPane, the
 *  transport-driven RightColumn (OutputCanvas owns the rAF/wasm loop), and the
 *  wired chrome (ToolbarWired/ActivityRailWired). A full-shell story would have
 *  to boot the wasm core and the network, so it isn't meaningful; the decoupled,
 *  wasm-free stories live on the leaf presentational components (Toolbar,
 *  ActivityRail, and the inspector panels) instead. */
export function Studio() {
  const state = useOpenSketch();
  const { dirty } = state;
  const sketchName = openContextLabel(state);
  return (
    <div className="studio">
      <ToolbarWired sketchName={sketchName} dirty={dirty} />
      <div className="studio-body">
        <ActivityRailWired />
        <EditorPane onSources={transport.setSources} />
        <RightColumn />
      </div>
    </div>
  );
}
