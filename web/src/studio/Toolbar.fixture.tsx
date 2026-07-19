import { Toolbar } from "./Toolbar";
import { sketchName } from "../fixtures";
import "./studio.css";

// Toolbar is presentational: sketch name / dirty / theme / handlers as props,
// with the wired AddSourceButton + WorkspaceActions injected as slots. Stories
// fill the slots with inert placeholder buttons so no wired child (transport /
// network) mounts — nothing touches the wasm core. Both themes render via
// Cosmos's root decorator sets the default theme on <html>.
const sourceSlot = (
  <button type="button" className="btn-ghost">
    + Source
  </button>
);
const workspaceSlot = (
  <button type="button" className="btn-ghost">
    Save
  </button>
);

const Clean = () => (
  <Toolbar sketchName={sketchName} dirty={false} theme="dark" sourceSlot={sourceSlot} workspaceSlot={workspaceSlot} />
);

const Dirty = () => (
  <Toolbar sketchName={sketchName} dirty theme="dark" sourceSlot={sourceSlot} workspaceSlot={workspaceSlot} />
);

export default {
  Clean,
  Dirty,
};
