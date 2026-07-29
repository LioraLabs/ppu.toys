import { makeSourceMeta, sourceMetaM7, sourcePayloadM7 } from "../fixtures";
import { CoreStage } from "../cosmos/FixtureStage";
import { AddSourceButton } from "./sources/AddSourceButton";
import { SourcePreview } from "./sources/SourcePreview";
import "./sources/sources.css";

export default (
  <CoreStage>
    <div style={{ display: "grid", gap: 16, width: 352, padding: 16 }}>
      <AddSourceButton />
      <SourcePreview
        kind="m7"
        meta={sourceMetaM7 ?? makeSourceMeta()}
        payload={sourcePayloadM7}
      />
    </div>
  </CoreStage>
);
