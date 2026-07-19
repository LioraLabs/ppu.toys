import { CoreStage } from "../../cosmos/FixtureStage";
import { EditorPane } from "../EditorPane";
import "../../styles/tokens.css";
import "../studio.css";

// Keep this composition wired to the production EditorPane, not a fixture copy.
export default (
  <CoreStage>
    <div style={{ display: "flex", width: "100%", height: "100vh" }}>
      <EditorPane onSources={() => ({ ok: true })} />
    </div>
  </CoreStage>
);
