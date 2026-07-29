import { useSyncExternalStore } from "react";
import type { TabId } from "./tabs";

export interface InspectorView {
  tab: TabId;
}

/** Shared inspector view state — which tab the INSPECTOR panel shows. (The
 *  panel's existence/placement belongs to the dock layout, not this store.) */
let state: InspectorView = { tab: "trace" };
const listeners = new Set<() => void>();

function set(next: InspectorView) {
  state = next;
  for (const l of listeners) l();
}

export const inspectorStore = {
  get: (): InspectorView => state,
  setTab(tab: TabId): void {
    set({ tab });
  },
  subscribe(cb: () => void): () => void {
    listeners.add(cb);
    return () => void listeners.delete(cb);
  },
  _resetForTests(): void {
    state = { tab: "trace" };
  },
};

export function useInspectorView(): InspectorView {
  return useSyncExternalStore(inspectorStore.subscribe, inspectorStore.get);
}
