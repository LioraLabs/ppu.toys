import { useSyncExternalStore } from "react";
import type { Asset } from "./assetStore";

/** Shared, app-wide list of uploaded assets. output/DropZone.tsx writes it;
 *  the VRAM inspector reads it. Decouples the two so previews/ids stay consistent. */
let assets: Asset[] = [];
const listeners = new Set<() => void>();

export const assetStore = {
  list: (): Asset[] => assets,
  add(a: Asset) {
    assets = [...assets, a];
    for (const l of listeners) l();
  },
  set(a: Asset) {
    const i = assets.findIndex((x) => x.id === a.id);
    assets = i === -1 ? [...assets, a] : assets.map((x) => (x.id === a.id ? a : x));
    for (const l of listeners) l();
  },
  /** Replace the whole list — opening a sketch/demo resets to its assets. */
  reset() {
    assets = [];
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
