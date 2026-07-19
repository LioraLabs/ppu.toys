import { MemoryRouter } from "react-router-dom";
import { OverlayStage } from "../../cosmos/FixtureStage";
import { PublishDialog } from "./PublishDialog";
import { publishSave } from "../../fixtures";
import "./cloud.css";

// PublishDialog rendered open. It is already prop-driven: the ensure-saved
// `save` fn is injected (WorkspaceActions passes the real one; the story passes
// a fixture stub that resolves a toy id offline). It reads the open-sketch store
// for the default title (wasm-free) and useNavigate on success, so it renders
// inside a MemoryRouter. The publish flow (save -> recordClip -> upload) only
// runs on the Publish click and is not exercised in a render-only story.
const noop = () => undefined;

// The scrim is position:fixed; OverlayStage contains it to the story pane so it
// bounds the fixed scrim to the fixture preview.
const Open = () => (
  <MemoryRouter>
    <OverlayStage>
      <PublishDialog onClose={noop} save={publishSave} />
    </OverlayStage>
  </MemoryRouter>
);

export default {
  Open,
};
