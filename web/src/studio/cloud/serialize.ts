import { openContextFiles, type OpenSketchState } from "../sketches/openSketch";
import { DEMOS } from "../demos/demos";
import { transport } from "../transport/transport";
import { encodeBase64 } from "../../api/base64";
import type { ToyFile, ToySource } from "../../api/apiClient";
import type { ConvertSourceResult } from "../../ppu/core";
import type { DemoAsset } from "../demos/demos";

/** Converts one bundled demo asset to a payload+meta. Injected in tests (which
 *  have no ImageData); the default builds ImageData and calls the live core —
 *  the SAME conversion loadDemo does, so payloads match what the demo renders. */
export type ConvertAsset = (asset: DemoAsset) => ConvertSourceResult;

/** Browser-only: ImageData exists at runtime, not in the vitest env. */
const defaultConvert: ConvertAsset = (a) =>
  transport.convertSource(a.kind, a.options, new ImageData(new Uint8ClampedArray(a.data), a.width, a.height));

/** The demo whose built-in art this workspace renders, if any: a demo context
 *  directly, or a sketch lazily forked from a demo (forkedFrom = demo id). A
 *  sketch forked from a CLOUD toy has forkedFrom = a toy slug (no DEMOS match)
 *  and is already self-contained in its own `sources`. */
function underlyingDemoId(state: OpenSketchState): string | undefined {
  const ctx = state.context;
  return ctx.kind === "demo" ? ctx.demoId : ctx.sketch.forkedFrom;
}

/** Serialize the open workspace for cloud save/publish: files verbatim, and EVERY
 *  rendered source as a payload-bearing record (demo/built-in art included), so the
 *  permalink player — which only replays addSource(payload) — renders it fully. */
export function serializeWorkspace(
  state: OpenSketchState,
  convert: ConvertAsset = defaultConvert,
): { files: ToyFile[]; sources: ToySource[] } {
  const files = openContextFiles(state).map((f) => ({ name: f.name, source: f.source }));
  const byName = new Map<string, ToySource>();

  const demoId = underlyingDemoId(state);
  const demo = demoId ? DEMOS.find((d) => d.id === demoId) : undefined;
  if (demo) {
    for (const a of demo.assets) {
      const { payload, meta } = convert(a);
      byName.set(a.id, { name: a.id, kind: a.kind, builtinId: null, options: a.options, meta, payload: encodeBase64(payload) });
    }
  }
  // user-added sources win over a same-named demo asset
  const userSources = state.context.kind === "sketch" ? state.context.sketch.sources : [];
  for (const s of userSources) {
    byName.set(s.name, { name: s.name, kind: s.kind, builtinId: null, options: s.options, meta: s.meta, payload: encodeBase64(s.payload) });
  }
  return { files, sources: [...byName.values()] };
}
