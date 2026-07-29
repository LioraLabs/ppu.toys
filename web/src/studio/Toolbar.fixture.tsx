import { Toolbar } from "./Toolbar";
import { sketchName } from "../fixtures";
import "./studio.css";

// Toolbar is presentational: toy name / dirty / theme / handlers as props,
// with the wired WorkspaceActions injected as a slot. Stories fill the slot
// with an inert placeholder button so no wired child (transport / network)
// mounts — nothing touches the wasm core.
const workspaceSlot = (
  <button type="button" className="btn-ghost">
    Save
  </button>
);

const Clean = () => (
  <Toolbar sketchName={sketchName} dirty={false} theme="dark" workspaceSlot={workspaceSlot} />
);

const Dirty = () => (
  <Toolbar sketchName={sketchName} dirty theme="dark" workspaceSlot={workspaceSlot} />
);

// Signed in: the account avatar renders (letter tile — no Discord hash in
// fixtures) and opens the profile/wall/sign-out menu.
const SignedIn = () => (
  <Toolbar
    sketchName={sketchName}
    theme="dark"
    user={{ id: "1", handle: "ada", avatar: null }}
   
    workspaceSlot={workspaceSlot}
  />
);

export default {
  Clean,
  Dirty,
  SignedIn,
};
