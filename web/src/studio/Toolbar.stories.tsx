import type { Story, StoryDefault } from "@ladle/react";
import { Toolbar } from "./Toolbar";
import { sketchName } from "../fixtures";
import "./studio.css";

// Toolbar is presentational: sketch name / dirty / theme / handlers as props,
// with the wired AddSourceButton + WorkspaceActions injected as slots. Stories
// fill the slots with inert placeholder buttons so no wired child (transport /
// network) mounts — nothing touches the wasm core. Both themes render via
// Ladle's global theme toolbar (data-theme on <html>).
export default {
  title: "Studio/Toolbar",
} satisfies StoryDefault;

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

export const Clean: Story = () => (
  <Toolbar sketchName={sketchName} dirty={false} theme="dark" sourceSlot={sourceSlot} workspaceSlot={workspaceSlot} />
);

export const Dirty: Story = () => (
  <Toolbar sketchName={sketchName} dirty theme="dark" sourceSlot={sourceSlot} workspaceSlot={workspaceSlot} />
);
