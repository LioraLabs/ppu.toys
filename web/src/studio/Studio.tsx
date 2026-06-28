import "../styles/tokens.css";
import "./studio.css";
import { Toolbar } from "./Toolbar";
import { ActivityRail } from "./ActivityRail";
import { LeftDock } from "./LeftDock";
import { EditorPane } from "./EditorPane";
import { RightColumn } from "./RightColumn";
import { StatusBar } from "./StatusBar";
import { transport } from "./transport/transport";

export function Studio() {
  return (
    <div className="studio">
      <Toolbar />
      <div className="studio-body">
        <ActivityRail />
        <LeftDock />
        <EditorPane onSource={transport.setSource} />
        <RightColumn />
      </div>
      <StatusBar />
    </div>
  );
}
