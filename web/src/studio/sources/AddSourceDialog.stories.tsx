import type { Story, StoryDefault } from "@ladle/react";
import { OverlayStage, withCore } from "../../../.ladle/decorators";
import { AddSourceDialog } from "./AddSourceDialog";
import "./sources.css";

// AddSourceDialog rendered open. The wasm core is only touched once an image is
// dropped (ppuCore.convertSource) — in its initial empty state it renders the
// drop zone, kind/depth controls and the preview hint with no wasm. This is the
// documented wasm-free render surface: the convert + transport.addSource paths
// need the real core and are exercised by AddSourceDialog.test.tsx, not here.
export default {
  title: "Studio/Sources/AddSourceDialog",
} satisfies StoryDefault;

const noop = () => undefined;

// The scrim is position:fixed; OverlayStage contains it to the story pane so it
// doesn't cover Ladle's sidebar (which would trap clicks on this story).
export const Open: Story = () => (
  <OverlayStage>
    <AddSourceDialog onClose={noop} />
  </OverlayStage>
);

// Live-core variant: the `withCore` decorator boots the REAL wasm PPU core before
// mounting, so dropping/choosing an image actually runs `ppuCore.convertSource`
// (real quantize + tile import) and the preview shows the genuine converted
// output — the wasm path AddSourceDialog.test.tsx stubs. This is the integration
// counterpart to the wasm-free `Open` story above.
export const OpenLiveCore: Story = () => (
  <OverlayStage>
    <AddSourceDialog onClose={noop} />
  </OverlayStage>
);
OpenLiveCore.decorators = [withCore];
