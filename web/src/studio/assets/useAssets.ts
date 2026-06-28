import { useCallback, useRef, useState } from "react";
import type { PpuCore } from "../../ppu/core";
import { Asset, registerAsset } from "./assetStore";
import { decodeImageFile, pngFiles } from "./decode";

/** Owns the uploaded-asset list and the drop/decode/register pipeline. */
export function useAssets(core: PpuCore) {
  const [assets, setAssets] = useState<Asset[]>([]);
  const [error, setError] = useState<string | null>(null);
  // Mirrors `assets` so sequential uploads dedupe against in-flight additions
  // without putting upload side effects inside a state updater.
  const ref = useRef<Asset[]>([]);

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
          const decoded = await decodeImageFile(file);
          const asset = registerAsset(core, ref.current, decoded);
          ref.current = [...ref.current, asset];
          setAssets(ref.current);
        } catch (e) {
          setError(e instanceof Error ? e.message : "Failed to decode image");
        }
      }
    },
    [core],
  );

  return { assets, error, addFiles };
}
