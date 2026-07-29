import { useSyncExternalStore } from "react";
import type { TabId } from "./tabs";

export interface InspectorView {
  tab: TabId;
  /** Bottom-dock visibility. Shared here (not StudioLayout-local) so the
   *  ActivityRail's view shortcuts can reveal the dock they target. */
  dockOpen: boolean;
}

const DOCK_OPEN_KEY = "ppu.dockOpen";

/** Shared inspector view state. Lifted out of the Inspector component so the
 *  ActivityRail (a sibling subtree) and the StudioLayout dock bar can drive
 *  the same view — rail "layers/sprites" are shortcuts into inspector tabs. */
let state: InspectorView = {
  tab: "trace",
  dockOpen: localStorage.getItem(DOCK_OPEN_KEY) !== "0",
};
const listeners = new Set<() => void>();

function set(next: InspectorView) {
  state = next;
  localStorage.setItem(DOCK_OPEN_KEY, state.dockOpen ? "1" : "0");
  for (const l of listeners) l();
}

export const inspectorStore = {
  get: (): InspectorView => state,
  /** Select a tab — and open the dock, so a rail shortcut always shows it. */
  setTab(tab: TabId): void {
    set({ tab, dockOpen: true });
  },
  toggleDock(): void {
    set({ ...state, dockOpen: !state.dockOpen });
  },
  subscribe(cb: () => void): () => void {
    listeners.add(cb);
    return () => void listeners.delete(cb);
  },
  _resetForTests(): void {
    state = { tab: "trace", dockOpen: true };
  },
};

export function useInspectorView(): InspectorView {
  return useSyncExternalStore(inspectorStore.subscribe, inspectorStore.get);
}
