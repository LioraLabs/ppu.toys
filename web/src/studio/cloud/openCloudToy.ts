import { decodeBase64 } from "../../api/base64";
import { createSketch, type SketchSource } from "../sketches/sketchStore";
import { openSketchStore } from "../sketches/openSketch";
import { cloudDraft } from "./cloudDraft";
import type { ToyFull } from "../../api/apiClient";
import type { SourceKind, ConvertSourceOptions, SourceMeta } from "../../ppu/core";

/** Load a cloud toy (own draft or a fresh fork) into the Studio: mint a local
 *  sketch from its files + payload-bearing sources, open it, and bind the cloud
 *  draft id to the resulting session so Save/Publish target the same server toy.
 *  The toy is self-contained (every source has a payload — see serializer), so
 *  no demo replay is needed: forkedFrom stays unset. */
export async function openCloudToy(toy: ToyFull): Promise<void> {
  const sources: SketchSource[] = toy.sources
    .filter((s) => s.payload)
    .map((s) => ({
      name: s.name,
      kind: s.kind as SourceKind,
      options: (s.options ?? {}) as ConvertSourceOptions,
      payload: decodeBase64(s.payload as string),
      meta: s.meta as SourceMeta,
    }));
  const sketch = await createSketch(toy.title || "untitled", toy.files, sources);
  await openSketchStore.openSketch(sketch.id);
  cloudDraft.set(toy.id, openSketchStore.state().session);
}
