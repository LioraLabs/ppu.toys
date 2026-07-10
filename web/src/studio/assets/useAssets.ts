import { useCallback, useState } from "react";
import { assetStore, useSharedAssets } from "./sharedAssets";
import { registerAsset } from "./assetStore";
import { decodeImageFile, pngFiles } from "./decode";
import { openSketchStore } from "../sketches/openSketch";

/** Owns the drop/decode/register pipeline. The asset LIST lives in the shared
 *  assetStore (so the VRAM tab sees it); uploads go through the supplied
 *  uploader (the transport, which pokes the shared core + refreshes the frame). */
export function useAssets(upload: (slot: string, image: ImageData) => void) {
  const assets = useSharedAssets();
  const [error, setError] = useState<string | null>(null);

  const addFiles = useCallback(
    async (files: FileList | File[]) => {
      const pngs = pngFiles(files);
      if (pngs.length === 0) {
        setError("Only PNG files are supported");
        return;
      }
      setError(null);
      for (const file of pngs) {
        try {
          const bytes = new Uint8Array(await file.arrayBuffer());
          const decoded = await decodeImageFile(file);
          const asset = registerAsset(upload, assetStore.list(), decoded);
          assetStore.add(asset);
          // persist the original bytes into the open sketch (forks a demo)
          openSketchStore.addAsset({ name: file.name, png: bytes });
        } catch (e) {
          setError(e instanceof Error ? e.message : "Failed to decode image");
        }
      }
    },
    [upload],
  );

  return { assets, error, addFiles };
}
