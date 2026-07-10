import init, { PpuCore as WasmCore } from "../wasm/pkg/ppu_core.js";
import {
  PpuCore,
  FrameResult,
  RegisterView,
  LuaError,
  OamSprite,
  ObjOverflow,
  AssetInfo,
  ImportReport,
  CompositorScreens,
  PlaneId,
  BgTrace,
  ObjTrace,
  PinnedRegister,
  WIDTH,
  HEIGHT,
} from "./core";

/** The slice of the wasm-bindgen core the adapter calls. Extracted so the adapter
 *  is unit-testable without instantiating the real wasm module. `frame()` returns
 *  void on success and THROWS a `{message, line?}` object on a Lua runtime error. */
export interface WasmCoreLike {
  setSource(src: string): unknown;
  frame(t: number, f: number): void;
  framebuffer(): ArrayLike<number>;
  registers(): unknown;
  cgram(): Uint16Array;
  vram?: () => Uint16Array;
  oam?: () => OamSprite[];
  objOverflow?: () => ObjOverflow;
  listAssets?: () => AssetInfo[];
  importReports?: () => ImportReport[];
  uploadTexture(slot: string, imageData: ImageData): void;
  setLayerVisible(id: string, visible: boolean): void;
  mainScreen?: () => ArrayLike<number>;
  subScreen?: () => ArrayLike<number>;
  mathMask?: () => ArrayLike<number>;
  layerView?: (plane: string) => ArrayLike<number>;
  traceBgPixel?: (layer: number, x: number, y: number) => unknown;
  traceBgTile?: (layer: number, tx: number, ty: number, y: number) => unknown;
  traceObj?: (index: number) => unknown;
  pinRegister?: (addr: number, value: number) => void;
  unpinRegister?: (addr: number) => void;
  clearPins?: () => void;
  listPins?: () => unknown;
}

/** `new Uint8ClampedArray(x)` has separate (length: number) / (data: ArrayLike<number>)
 *  overloads, so a `data ?? size` union doesn't resolve to either — these pick the
 *  right overload explicitly. */
function clamped(data: ArrayLike<number> | undefined, size: number): Uint8ClampedArray {
  return data ? new Uint8ClampedArray(data) : new Uint8ClampedArray(size);
}
function bytes(data: ArrayLike<number> | undefined, size: number): Uint8Array {
  return data ? new Uint8Array(data) : new Uint8Array(size);
}

/** Adapt a wasm-bindgen core to the PpuCore seam. Pure (no wasm load) so it can be
 *  unit-tested. On a Lua runtime error `core.frame` throws; we let it propagate so
 *  the transport's safeFrame surfaces it as an editor diagnostic. */
export function wrapWasmCore(core: WasmCoreLike): PpuCore {
  return {
    setSource(src: string) {
      return core.setSource(src) as { ok: boolean; error?: LuaError };
    },
    frame(t: number, f: number): FrameResult {
      core.frame(t, f); // throws on Lua runtime error -> transport.safeFrame surfaces it
      return {
        framebuffer: new Uint8ClampedArray(core.framebuffer()),
        registers: core.registers() as RegisterView[],
        cgram: core.cgram(),
        oam: core.oam?.() ?? [],
        objOverflow: (core.objOverflow?.() as ObjOverflow) ?? {
          rangeOver: false,
          timeOver: false,
          maxSprites: 0,
          maxTiles: 0,
        },
      };
    },
    uploadTexture(slot: string, imageData: ImageData) {
      core.uploadTexture(slot, imageData);
    },
    setLayerVisible(id: string, visible: boolean) {
      core.setLayerVisible(id, visible);
    },
    listAssets(): AssetInfo[] {
      return core.listAssets?.() ?? [];
    },
    vram(): Uint16Array {
      return core.vram?.() ?? new Uint16Array(0);
    },
    importReports(): ImportReport[] {
      return core.importReports?.() ?? [];
    },
    screens(): CompositorScreens {
      return {
        main: clamped(core.mainScreen?.(), WIDTH * HEIGHT * 4),
        sub: clamped(core.subScreen?.(), WIDTH * HEIGHT * 4),
        mathMask: bytes(core.mathMask?.(), WIDTH * HEIGHT),
      };
    },
    layerView(plane: PlaneId): Uint8ClampedArray {
      return clamped(core.layerView?.(plane), WIDTH * HEIGHT * 4);
    },
    traceBgPixel(layer: number, x: number, y: number): BgTrace | null {
      return (core.traceBgPixel?.(layer, x, y) as BgTrace | null | undefined) ?? null;
    },
    traceBgTile(layer: number, tx: number, ty: number, y: number): BgTrace | null {
      return (core.traceBgTile?.(layer, tx, ty, y) as BgTrace | null | undefined) ?? null;
    },
    traceObj(index: number): ObjTrace | null {
      return (core.traceObj?.(index) as ObjTrace | null | undefined) ?? null;
    },
    pin(addr: number, value: number) {
      core.pinRegister?.(addr, value);
    },
    unpin(addr: number) {
      core.unpinRegister?.(addr);
    },
    clearPins() {
      core.clearPins?.();
    },
    listPins(): PinnedRegister[] {
      return (core.listPins?.() as PinnedRegister[] | undefined) ?? [];
    },
  };
}

/** Load the wasm-pack module and adapt it to the PpuCore interface. */
export async function createWasmPpuCore(): Promise<PpuCore> {
  await init();
  return wrapWasmCore(new WasmCore() as unknown as WasmCoreLike);
}
