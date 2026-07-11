import { useCallback, useState } from "react";
import { assetStore, useSharedAssets } from "./sharedAssets";
import { assetId } from "./assetStore";
import { decodeImageFile, pngFiles } from "./decode";
import { transport } from "../transport/transport";
import { openSketchStore } from "../sketches/openSketch";

const DEFAULT_KIND = "bg" as const;
const DEFAULT_OPTIONS = { bit_depth: 4 } as const;

/** Owns the drop/convert/register pipeline. Dropped PNGs are quantized to a
 *  format-committed source (default bg/4bpp — the kind/options picker is a
 *  later ticket), registered for rendering, and persisted into the open
 *  sketch as a source. */
export function useAssets() {
  const assets = useSharedAssets();
  const [error, setError] = useState<string | null>(null);

  const addFiles = useCallback(async (files: FileList | File[]) => {
    const pngs = pngFiles(files);
    if (pngs.length === 0) {
      setError("Only PNG files are supported");
      return;
    }
    setError(null);
    for (const file of pngs) {
      try {
        const decoded = await decodeImageFile(file);
        const name = assetId(file.name, assetStore.list().map((a) => a.id));
        const { payload, meta } = transport.convertSource(DEFAULT_KIND, DEFAULT_OPTIONS, decoded.imageData);
        transport.addSource(name, payload);
        assetStore.set({ id: name, name, width: meta.width, height: meta.height, preview: decoded.preview });
        openSketchStore.addSource({ name, kind: DEFAULT_KIND, options: DEFAULT_OPTIONS, payload, meta });
      } catch (e) {
        setError(e instanceof Error ? e.message : "Failed to decode image");
      }
    }
  }, []);

  return { assets, error, addFiles };
}
