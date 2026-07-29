import { makeSourceMeta, sourceMetaM7, sourcePayloadM7 } from "../fixtures";
import { CoreStage, OverlayStage } from "../cosmos/FixtureStage";
import { AssetsPanel } from "./sources/AssetsPanel";
import { SourcePreview } from "./sources/SourcePreview";
import "./sources/sources.css";

// The production sources composition: the rail's Assets flyout (which owns the
// add dialog) beside a decoded-source preview. CoreStage because AssetsPanel's
// add dialog converts through the real core, and the panel reads the live
// open-sketch store.
export default (
  <CoreStage>
    <OverlayStage>
      <div style={{ display: "grid", gap: 16, width: 352, padding: 16 }}>
        <SourcePreview
          kind="m7"
          meta={sourceMetaM7 ?? makeSourceMeta()}
          payload={sourcePayloadM7}
        />
      </div>
      <AssetsPanel onClose={() => {}} />
    </OverlayStage>
  </CoreStage>
);
