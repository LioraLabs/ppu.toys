import "../styles/tokens.css";
import "./studio.css";
import { Toolbar } from "./Toolbar";
import { ActivityRail } from "./ActivityRail";
import { EditorPane } from "./EditorPane";
import { RightColumn } from "./RightColumn";
import { transport } from "./transport/transport";
import { useOpenSketch, openContextLabel } from "./sketches/openSketch";

export function Studio() {
  const state = useOpenSketch();
  const { dirty } = state;
  const sketchName = openContextLabel(state);
  return (
    <div className="studio">
      <Toolbar sketchName={sketchName} dirty={dirty} />
      <div className="studio-body">
        <ActivityRail />
        <EditorPane onSources={transport.setSources} />
        <RightColumn />
      </div>
    </div>
  );
}
