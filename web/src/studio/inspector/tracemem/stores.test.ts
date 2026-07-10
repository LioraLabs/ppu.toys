import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  layerVis,
  pickPaletteIdx,
  resetTraceSelection,
  selectObj,
  selectPixel,
  selectPlane,
  setLayerVisible,
  traceSelection,
} from "./stores";

beforeEach(() => resetTraceSelection());

describe("traceSelection store", () => {
  it("starts at the screen center on BG1", () => {
    expect(traceSelection.get()).toEqual({ plane: "bg1", x: 128, y: 112, objIndex: 0, pickedIdx: null });
  });
  it("selecting a plane/pixel/sprite clears the palette pick", () => {
    pickPaletteIdx(7);
    expect(traceSelection.get().pickedIdx).toBe(7);
    selectPlane("bg2");
    expect(traceSelection.get()).toMatchObject({ plane: "bg2", pickedIdx: null });
    pickPaletteIdx(3);
    selectPixel(10, 20);
    expect(traceSelection.get()).toMatchObject({ x: 10, y: 20, pickedIdx: null });
    pickPaletteIdx(3);
    selectObj(42);
    expect(traceSelection.get()).toMatchObject({ objIndex: 42, pickedIdx: null });
  });
  it("notifies subscribers and keeps snapshot identity stable between writes", () => {
    const cb = vi.fn();
    const un = traceSelection.subscribe(cb);
    const before = traceSelection.get();
    expect(traceSelection.get()).toBe(before); // stable identity for useSyncExternalStore
    selectPixel(1, 2);
    expect(cb).toHaveBeenCalledTimes(1);
    expect(traceSelection.get()).not.toBe(before);
    un();
    selectPixel(3, 4);
    expect(cb).toHaveBeenCalledTimes(1);
  });
});

describe("layer visibility mirror", () => {
  it("routes through the transport setter and mirrors the state", () => {
    const t = { setLayerVisible: vi.fn() };
    setLayerVisible("bg2", false, t);
    expect(t.setLayerVisible).toHaveBeenCalledWith("bg2", false);
    expect(layerVis.get().bg2).toBe(false);
    setLayerVisible("bg2", true, t);
    expect(layerVis.get().bg2).toBe(true);
  });
});
