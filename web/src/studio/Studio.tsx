import "../styles/tokens.css";
import "./studio.css";
import { Toolbar } from "./Toolbar";
import { ActivityRail } from "./ActivityRail";
import { EditorPane } from "./EditorPane";
import { RightColumn } from "./RightColumn";
import { transport } from "./transport/transport";
import { useOpenSketch } from "./sketches/openSketch";
import { DEMOS } from "./demos/demos";

export function Studio() {
  const { context, dirty } = useOpenSketch();
  const sketchName =
    context.kind === "sketch"
      ? context.sketch.name
      : (DEMOS.find((d) => d.id === context.demoId)?.label ?? context.demoId);
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
