import { useSyncExternalStore } from "react";
import type { Asset } from "./assetStore";

/** Shared, app-wide list of uploaded assets. AssetsPanel writes it; the VRAM
 *  inspector reads it. Decouples the two so previews/ids stay consistent. */
let assets: Asset[] = [];
const listeners = new Set<() => void>();

export const assetStore = {
  list: (): Asset[] => assets,
  add(a: Asset) {
    assets = [...assets, a];
    for (const l of listeners) l();
  },
  subscribe(cb: () => void) {
    listeners.add(cb);
    return () => listeners.delete(cb);
  },
};

export function useSharedAssets(): Asset[] {
  return useSyncExternalStore(assetStore.subscribe, assetStore.list);
}
