import { MemoryRouter } from "react-router-dom";
import type { Story, StoryDefault } from "@ladle/react";
import { PublishDialog } from "./PublishDialog";
import { publishSave } from "../../fixtures";
import "./cloud.css";

// PublishDialog rendered open. It is already prop-driven: the ensure-saved
// `save` fn is injected (WorkspaceActions passes the real one; the story passes
// a fixture stub that resolves a toy id offline). It reads the open-sketch store
// for the default title (wasm-free) and useNavigate on success, so it renders
// inside a MemoryRouter. The publish flow (save -> recordClip -> upload) only
// runs on the Publish click and is not exercised in a render-only story.
export default {
  title: "Studio/Cloud/PublishDialog",
} satisfies StoryDefault;

const noop = () => undefined;

// The scrim is position:fixed, so give #ladle-root a viewport-sized in-flow box
// (otherwise it collapses to 0 height and the screenshot target is "not visible").
export const Open: Story = () => (
  <MemoryRouter>
    <div style={{ minHeight: "100vh" }}>
      <PublishDialog onClose={noop} save={publishSave} />
    </div>
  </MemoryRouter>
);
