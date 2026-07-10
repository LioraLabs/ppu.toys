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
  SourceFile,
} from "./core";

/** The slice of the wasm-bindgen core the adapter calls. Extracted so the adapter
 *  is unit-testable without instantiating the real wasm module. `frame()` returns
 *  void on success and THROWS a `{message, line?, file?}` object on a Lua runtime error. */
export interface WasmCoreLike {
  setSource(src: string): unknown;
  setSources?: (files: SourceFile[]) => unknown;
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
      if (core.setSources) {
        return core.setSources(files) as { ok: boolean; error?: LuaError };
      }
      // Older wasm module: concatenation keeps the shared-global semantics
      // (only per-file error attribution is lost).
      return core.setSource(files.map((f) => f.source).join("\n")) as {
        ok: boolean;
        error?: LuaError;
      };
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
  };
}

/** Load the wasm-pack module and adapt it to the PpuCore interface. */
export async function createWasmPpuCore(): Promise<PpuCore> {
  await init();
  return wrapWasmCore(new WasmCore() as unknown as WasmCoreLike);
}
