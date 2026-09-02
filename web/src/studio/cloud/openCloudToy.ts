import { decodeBase64 } from "../../api/base64";
import {
  createSketch,
  type SketchSource,
  type SketchFile,
  type SketchOrigin,
} from "../sketches/sketchStore";
import { openSketchStore } from "../sketches/openSketch";
import type { ToyFull } from "../../api/apiClient";
import type { SourceKind, ConvertSourceOptions, SourceMeta } from "../../ppu/core";

/** Mint a sketch from files + sources, open it, and link it to `origin` (if
 *  given) — the shared tail for opening any external toy or file into the
 *  Studio. Flushes immediately so a reload right after opening still shows
 *  the link. */
export async function openAsSketch(
  name: string,
  files: SketchFile[],
  sources: SketchSource[],
  origin?: SketchOrigin,
): Promise<void> {
  const sketch = await createSketch(name, files, sources);
  await openSketchStore.openSketch(sketch.id);
  if (origin) openSketchStore.setOrigin(origin);
  await openSketchStore.flush();
}

/** Open a published toy from its permalink into the Studio — the owner's own
 *  (Edit) or someone else's (Fork): mint a local sketch from its files +
 *  payload-bearing sources, open it, and set its origin so Save/Publish target
 *  the same server toy. The toy is self-contained (every source has a
 *  payload — see serializer), so no demo replay is needed: forkedFrom stays
 *  unset. */
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
  await openAsSketch(toy.title || "untitled", toy.files, sources, {
    id: toy.id,
    revision: toy.revision,
    authorId: toy.author.id,
  });
}
