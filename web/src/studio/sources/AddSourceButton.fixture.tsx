import { AddSourceButton } from "./AddSourceButton";
import { CoreStage } from "../../cosmos/FixtureStage";
import "./sources.css";

// AddSourceButton is a self-contained toggle: it renders the "+ Source" ghost
// button and mounts AddSourceDialog on click. Rendered closed here (the button
// alone); clicking it mounts AddSourceDialog, so boot the real core before the
// conversion interaction is reachable.
const Default = () => (
  <CoreStage>
    <div style={{ padding: 16 }}>
      <AddSourceButton />
    </div>
  </CoreStage>
);

export default {
  Default,
};
