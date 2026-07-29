import { useSyncExternalStore } from "react";
import type { OverlayId, TabId } from "./tabs";

export interface InspectorView {
  tab: TabId;
  overlay: OverlayId | null;
}

/** Shared inspector view state. Lifted out of the Inspector component so the
 *  ActivityRail (a sibling subtree) can drive tab selection and overlays —
 *  rail "layers/palette/sprites" are shortcuts into inspector views. */
let state: InspectorView = { tab: "trace", overlay: null };
const listeners = new Set<() => void>();

function set(next: InspectorView) {
  state = next;
  for (const l of listeners) l();
}

export const inspectorStore = {
  get: (): InspectorView => state,
  /** Select a tab (and drop any overlay so the tab is actually visible). */
  setTab(tab: TabId): void {
    set({ tab, overlay: null });
  },
  openOverlay(overlay: OverlayId): void {
    set({ ...state, overlay });
  },
  collapse(): void {
    set({ ...state, overlay: null });
  },
  subscribe(cb: () => void): () => void {
    listeners.add(cb);
    return () => void listeners.delete(cb);
  },
  _resetForTests(): void {
    state = { tab: "trace", overlay: null };
  },
};

export function useInspectorView(): InspectorView {
  return useSyncExternalStore(inspectorStore.subscribe, inspectorStore.get);
}
