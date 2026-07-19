import { CoreStage } from "../../cosmos/FixtureStage";
import "../../styles/tokens.css";
import "../studio.css";
import { Inspector } from "./Inspector";

// Keep this composition wired to the production Inspector, not a fixture copy.
export default (
  <CoreStage>
    <div style={{ display: "flex", width: "100%", height: "100vh" }}>
      <Inspector />
    </div>
  </CoreStage>
);
