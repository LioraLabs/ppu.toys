import { openContextFiles, type OpenSketchState } from "../sketches/openSketch";
import { encodeBase64 } from "../../api/base64";
import type { ToyFile, ToySource } from "../../api/apiClient";

/** Serialize the open workspace for cloud save/publish: files verbatim, and EVERY
 *  rendered source as a payload-bearing record (demo/built-in art included), so the
 *  permalink player — which only replays addSource(payload) — renders it fully. */
export function serializeWorkspace(state: OpenSketchState): {
  files: ToyFile[];
  sources: ToySource[];
} {
  const files = openContextFiles(state).map((f) => ({ name: f.name, source: f.source }));
  const byName = new Map<string, ToySource>();

  const userSources = state.context.sketch.sources;
  for (const s of userSources) {
    byName.set(s.name, {
      name: s.name,
      kind: s.kind,
      builtinId: null,
      options: s.options,
      meta: s.meta,
      payload: encodeBase64(s.payload),
    });
  }
  return { files, sources: [...byName.values()] };
}
