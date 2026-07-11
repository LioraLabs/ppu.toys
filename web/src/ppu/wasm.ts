import init, { PpuCore as WasmCore } from "../wasm/pkg/ppu_core.js";
import {
  PpuCore,
  FrameResult,
  RegisterView,
  LuaError,
  OamSprite,
  ObjOverflow,
  ImportReport,
  SourceFile,
  CompositorScreens,
  PlaneId,
  BgTrace,
  ObjTrace,
  SourceKind,
  ConvertSourceOptions,
  ConvertSourceResult,
} from "./core";

/** The slice of the wasm-bindgen core the adapter calls. Extracted so the adapter
 *  is unit-testable without instantiating the real wasm module. `frame()` returns
 *  void on success and THROWS a `{message, line?, file?}` object on a Lua runtime error.
 *
 *  Every method is REQUIRED: the pkg is always built from this same tree
 *  (content-cached `cook wasm`), so an "older wasm module" cannot exist — a
 *  contract mismatch must fail loudly at this boundary, not degrade silently. */
export interface WasmCoreLike {
  setSource(src: string): unknown;
  setSources(files: SourceFile[]): unknown;
  frame(t: number, f: number): void;
  framebuffer(): ArrayLike<number>;
  registers(): unknown;
  cgram(): Uint16Array;
  vram(): Uint16Array;
  oam(): OamSprite[];
  objOverflow(): ObjOverflow;
  importReports(): ImportReport[];
  setLayerVisible(id: string, visible: boolean): void;
  mainScreen(): ArrayLike<number>;
  subScreen(): ArrayLike<number>;
  mathMask(): ArrayLike<number>;
  layerView(plane: string): ArrayLike<number>;
  traceBgPixel(layer: number, x: number, y: number): unknown;
  traceBgTile(layer: number, tx: number, ty: number, y: number): unknown;
  traceObj(index: number): unknown;
  convertSource(kind: SourceKind, options: ConvertSourceOptions, imageData: ImageData): unknown;
  addSource(name: string, payload: Uint8Array): unknown;
}

/** Adapt a wasm-bindgen core to the PpuCore seam. Pure (no wasm load) so it can be
 *  unit-tested. On a Lua runtime error `core.frame` throws; we let it propagate so
 *  the transport's safeFrame surfaces it as an editor diagnostic. */
export function wrapWasmCore(core: WasmCoreLike): PpuCore {
  return {
    setSource(src: string) {
      return core.setSource(src) as { ok: boolean; error?: LuaError };
    },
    setSources(files: SourceFile[]) {
      return core.setSources(files) as { ok: boolean; error?: LuaError };
    },
    frame(t: number, f: number): FrameResult {
      core.frame(t, f); // throws on Lua runtime error -> transport.safeFrame surfaces it
      return {
        framebuffer: new Uint8ClampedArray(core.framebuffer()),
        registers: core.registers() as RegisterView[],
        cgram: core.cgram(),
        oam: core.oam(),
        objOverflow: core.objOverflow(),
      };
    },
    setLayerVisible(id: string, visible: boolean) {
      core.setLayerVisible(id, visible);
    },
    vram(): Uint16Array {
      return core.vram();
    },
    importReports(): ImportReport[] {
      return core.importReports();
    },
    screens(): CompositorScreens {
      return {
        main: new Uint8ClampedArray(core.mainScreen()),
        sub: new Uint8ClampedArray(core.subScreen()),
        mathMask: new Uint8Array(core.mathMask()),
      };
    },
    layerView(plane: PlaneId): Uint8ClampedArray {
      return new Uint8ClampedArray(core.layerView(plane));
    },
    // trace results: wasm-bindgen serializes a Rust `None` as undefined — the
    // `?? null` below normalizes the VALUE, the methods themselves are required.
    traceBgPixel(layer: number, x: number, y: number): BgTrace | null {
      return (core.traceBgPixel(layer, x, y) as BgTrace | null | undefined) ?? null;
    },
    traceBgTile(layer: number, tx: number, ty: number, y: number): BgTrace | null {
      return (core.traceBgTile(layer, tx, ty, y) as BgTrace | null | undefined) ?? null;
    },
    traceObj(index: number): ObjTrace | null {
      return (core.traceObj(index) as ObjTrace | null | undefined) ?? null;
    },
    convertSource(kind: SourceKind, options: ConvertSourceOptions, imageData: ImageData): ConvertSourceResult {
      return core.convertSource(kind, options, imageData) as ConvertSourceResult;
    },
    addSource(name: string, payload: Uint8Array) {
      return core.addSource(name, payload) as { ok: boolean; error?: string };
    },
  };
}

/** Load the wasm-pack module and adapt it to the PpuCore interface. */
export async function createWasmPpuCore(): Promise<PpuCore> {
  await init();
  return wrapWasmCore(new WasmCore() as unknown as WasmCoreLike);
}
