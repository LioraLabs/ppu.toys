import { SourcePreview } from "./SourcePreview";
import { sourceMetaM7, sourcePayloadM7, makeSourceMeta } from "../../fixtures";
import "./sources.css";

// SourcePreview is already pure props: given a source kind + meta + payload it
// decodes and renders the quantized preview, per-cell labels and budget chips
// with no wasm. The m7 fixture payload decodes to a real 16x16 image.
const Mode7 = () => (
  <div style={{ width: 320, padding: 16 }}>
    <SourcePreview kind="m7" meta={sourceMetaM7} payload={sourcePayloadM7} />
  </div>
);

// Overflow warning path: a report that exceeds the tile budget surfaces a warn
// chip. Payload is empty so the preview degrades to the labelled grid + budget.
const WithOverflowWarning = () => (
  <div style={{ width: 320, padding: 16 }}>
    <SourcePreview
      kind="m7"
      meta={makeSourceMeta({
        report: {
          mode: "m7",
          report: {
            colors: 200,
            unique_tiles: 300,
            tile_capacity: 256,
            overflow_tiles: 44,
            map_tiles_w: 2,
            map_tiles_h: 2,
          },
        },
      })}
      payload={new Uint8Array()}
    />
  </div>
);

export default {
  Mode7,
  WithOverflowWarning,
};
