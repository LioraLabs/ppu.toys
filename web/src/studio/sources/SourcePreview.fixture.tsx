import { SourcePreview } from "./SourcePreview";
import { sourceMetaBg, sourceMetaM7, sourcePayloadBg, sourcePayloadM7, makeSourceMeta } from "../../fixtures";
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

// Two sub-palettes: exercises the swatch rows, the palette tint overlay
// toggle, and the snes/source A/B chips (a solid source ImageData stands in
// for the original drop).
const BgTwoPalettes = () => {
  const src = new ImageData(16, 8);
  for (let i = 0; i < src.data.length; i += 4) {
    const left = (i / 4) % 16 < 8;
    src.data.set(left ? [255, 40, 40, 255] : [40, 40, 255, 255], i);
  }
  return (
    <div style={{ width: 320, padding: 16 }}>
      <SourcePreview kind="bg" meta={sourceMetaBg} payload={sourcePayloadBg} sourceImage={src} />
    </div>
  );
};

export default {
  Mode7,
  BgTwoPalettes,
  WithOverflowWarning,
};
