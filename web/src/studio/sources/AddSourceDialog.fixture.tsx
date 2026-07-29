import { CoreStage, OverlayStage } from "../../cosmos/FixtureStage";
import { AddSourceDialog } from "./AddSourceDialog";
import "./sources.css";

// AddSourceDialog rendered open. Dropping an image calls ppuCore.convertSource,
// so every interactive variant boots the real core before mounting the dialog.
const noop = () => undefined;

// The scrim is position:fixed; OverlayStage contains it to the story pane so it
// bounds the fixed scrim to the fixture preview.
const Open = () => (
  <CoreStage>
    <OverlayStage>
      <AddSourceDialog onClose={noop} />
    </OverlayStage>
  </CoreStage>
);

// Live-core variant: `CoreStage` boots the REAL wasm PPU core before
// mounting, so dropping/choosing an image actually runs `ppuCore.convertSource`
// (real quantize + tile import) and the preview shows the genuine converted
// output — the wasm path AddSourceDialog.test.tsx stubs. This is the integration
// counterpart to the wasm-free `Open` story above.
const OpenLiveCore = () => (
  <CoreStage>
    <OverlayStage>
      <AddSourceDialog onClose={noop} />
    </OverlayStage>
  </CoreStage>
);

export default {
  Open,
  OpenLiveCore,
};
