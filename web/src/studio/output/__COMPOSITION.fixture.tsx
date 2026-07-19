import { CoreStage } from "../../cosmos/FixtureStage";
import { OutputCanvas } from "./OutputCanvas";
import "../studio.css";

// The real WASM-backed output composition used by Studio.
export default (
  <CoreStage>
    <div style={{ width: 620, padding: 16 }}>
      <OutputCanvas />
    </div>
  </CoreStage>
);
