import { transport } from "../transport/transport";
import { assetStore } from "../assets/sharedAssets";
import { DEMOS } from "../demos/demos";
import { loadDemo } from "../demos/loadDemo";
import type { OpenContext } from "./openSketch";

/** Load an open context's graphics into the live core + shared list. A forked
 *  demo replays its procedural assets first (literal ids), then the sketch's
 *  stored source payloads register by name via the core's addSource —
 *  reproducing render state without decoding any PNG. Fully synchronous: no
 *  await window, so overlapping opens can't interleave their list mutations. */
export function restoreOpenContext(ctx: OpenContext): void {
  assetStore.reset();
  if (ctx.kind === "demo") {
    const demo = DEMOS.find((d) => d.id === ctx.demoId);
    if (demo) loadDemo(demo);
    return;
  }
  const from = ctx.sketch.forkedFrom ? DEMOS.find((d) => d.id === ctx.sketch.forkedFrom) : undefined;
  if (from) loadDemo(from);
  for (const s of ctx.sketch.sources) {
    transport.addSource(s.name, s.payload);
    assetStore.set({ id: s.name, name: s.name, width: s.meta.width, height: s.meta.height, preview: "" });
  }
}
