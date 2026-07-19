import { StudioLayout } from "./StudioLayout";
import { ToolbarWired } from "./ToolbarWired";
import { ActivityRailWired } from "./ActivityRailWired";
import { EditorPane } from "./EditorPane";
import { RightColumn } from "./RightColumn";
import { transport } from "./transport/transport";
import { useOpenSketch, openContextLabel } from "./sketches/openSketch";

/** The wired studio: fills StudioLayout's slots with the full app — the sketch
 *  store (openSketch), the CodeMirror EditorPane, the transport-driven
 *  RightColumn (OutputCanvas owns the rAF/wasm loop), and the wired chrome
 *  (ToolbarWired/ActivityRailWired). The composition itself is available
 *  wasm-free via StudioLayout.fixture (fixture-fed slots); this wired shell is
 *  available there too as the opt-in `CoreStage` live fixture. */
export function Studio() {
  const state = useOpenSketch();
  const { dirty } = state;
  const sketchName = openContextLabel(state);
  return (
    <StudioLayout
      toolbar={<ToolbarWired sketchName={sketchName} dirty={dirty} />}
      rail={<ActivityRailWired />}
      editor={<EditorPane onSources={transport.setSources} />}
      right={<RightColumn />}
    />
  );
}
