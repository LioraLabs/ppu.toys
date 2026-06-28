import { transport } from "../transport/transport";
import { assetStore } from "../assets/sharedAssets";
import type { Demo, DemoAsset } from "./demos";

/** data-URL thumbnail for the ASSETS panel. Browser-only (uses a 2D canvas). */
function preview(image: ImageData): string {
  const canvas = document.createElement("canvas");
  canvas.width = image.width;
  canvas.height = image.height;
  const ctx = canvas.getContext("2d");
  if (!ctx) return "";
  ctx.putImageData(image, 0, 0);
  return canvas.toDataURL("image/png");
}

function toImageData(a: DemoAsset): ImageData {
  return new ImageData(new Uint8ClampedArray(a.data), a.width, a.height);
}

/** Push a demo's bundled sources into the live core and the shared asset store,
 *  using the demo's literal slot ids (not slugified) so the Lua references
 *  resolve. The editor doc swap (which triggers setSource) is the caller's job. */
export function loadDemo(demo: Demo, upload = transport.uploadTexture): void {
  for (const a of demo.assets) {
    const image = toImageData(a);
    upload(a.id, image);
    assetStore.set({ id: a.id, name: `${a.id} · demo`, width: a.width, height: a.height, preview: preview(image) });
  }
}
