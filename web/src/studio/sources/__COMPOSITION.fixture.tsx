import { makeSourceMeta, sourceMetaM7, sourcePayloadM7 } from "../../fixtures";
import { AddSourceButton } from "./AddSourceButton";
import { SourcePreview } from "./SourcePreview";
import "./sources.css";

export default (
  <div style={{ display: "grid", gap: 16, width: 352, padding: 16 }}>
    <AddSourceButton />
    <SourcePreview
      kind="m7"
      meta={sourceMetaM7 ?? makeSourceMeta()}
      payload={sourcePayloadM7}
    />
  </div>
);
