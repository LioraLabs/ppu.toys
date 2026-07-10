import { transport } from "../transport/transport";
import { assetStore } from "../assets/sharedAssets";
import { registerAsset } from "../assets/assetStore";
import { decodeImageBlob } from "../assets/decode";
import { DEMOS } from "../demos/demos";
import { loadDemo } from "../demos/loadDemo";
import type { OpenContext } from "./openSketch";

/** Load an open context's assets into the live core + shared asset list.
 *  Browser-only (PNG decode via canvas).
 *
 *  Determinism contract: the shared list is reset, the forked-from demo's
 *  procedural assets replay first (literal ids), then the sketch's stored
 *  PNGs register in array order through the same registerAsset/assetId
 *  dedupe as the original uploads — reproducing the exact ids the sketch's
 *  Lua source was written against. */
export async function restoreOpenContext(ctx: OpenContext): Promise<void> {
  assetStore.reset();
  if (ctx.kind === "demo") {
    const demo = DEMOS.find((d) => d.id === ctx.demoId);
    if (demo) loadDemo(demo);
    return;
  }
  const from = ctx.sketch.forkedFrom
    ? DEMOS.find((d) => d.id === ctx.sketch.forkedFrom)
    : undefined;
  if (from) loadDemo(from);
  for (const a of ctx.sketch.assets) {
    const blob = new Blob([a.png as BlobPart], { type: "image/png" });
    const decoded = await decodeImageBlob(blob, a.name);
    const asset = registerAsset(transport.uploadTexture, assetStore.list(), decoded);
    assetStore.add(asset);
  }
}
