import { useSyncExternalStore } from "react";
import type { PlaneId } from "../../../ppu/core";
import { transport } from "../../transport/transport";
import type { TracePlane } from "./trace";

/** Tiny external store (house pattern: useSyncExternalStore, stable snapshot). */
class Store<T extends object> {
  private listeners = new Set<() => void>();
  constructor(private state: T) {}
  get = (): T => this.state;
  set = (patch: Partial<T>) => {
    this.state = { ...this.state, ...patch };
    for (const l of this.listeners) l();
  };
  subscribe = (cb: () => void) => {
    this.listeners.add(cb);
    return () => void this.listeners.delete(cb);
  };
}

export interface TraceSelection {
  plane: TracePlane;
  x: number; // BG screen-pixel selection
  y: number;
  objIndex: number; // OBJ selection
  pickedIdx: number | null; // palette-strip pick override (null = follow pixel)
}

const INITIAL: TraceSelection = { plane: "bg1", x: 128, y: 112, objIndex: 0, pickedIdx: null };

/** Shared by TraceTab AND the Memory & Layers overlay (same selection both places). */
export const traceSelection = new Store<TraceSelection>({ ...INITIAL });

export const useTraceSelection = () =>
  useSyncExternalStore(traceSelection.subscribe, traceSelection.get);

export const selectPlane = (plane: TracePlane) => traceSelection.set({ plane, pickedIdx: null });
export const selectPixel = (x: number, y: number) => traceSelection.set({ x, y, pickedIdx: null });
export const selectObj = (objIndex: number) => traceSelection.set({ objIndex, pickedIdx: null });
export const pickPaletteIdx = (pickedIdx: number | null) => traceSelection.set({ pickedIdx });
export const resetTraceSelection = () => traceSelection.set({ ...INITIAL });

/** Layer visibility (relocated from the old LeftDock into the overlay). The core
 *  has no visibility getter, so this mirror is the UI's source of truth; writes
 *  route through the transport so the shared core re-renders. */
export type LayerVis = Record<PlaneId, boolean>;

export const layerVis = new Store<LayerVis>({ bg1: true, bg2: true, bg3: true, bg4: true, obj: true });

export const useLayerVis = () => useSyncExternalStore(layerVis.subscribe, layerVis.get);

export function setLayerVisible(
  id: PlaneId,
  visible: boolean,
  t: { setLayerVisible: (id: string, visible: boolean) => void } = transport,
) {
  t.setLayerVisible(id, visible);
  layerVis.set({ [id]: visible } as Partial<LayerVis>);
}
