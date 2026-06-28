import init, { PpuCore as WasmCore } from "../wasm/pkg/ppu_core.js";
import { PpuCore, FrameResult, RegisterView, LuaError, OamSprite, AssetInfo } from "./core";

/** The slice of the wasm-bindgen core the adapter calls. Extracted so the adapter
 *  is unit-testable without instantiating the real wasm module. `frame()` returns
 *  void on success and THROWS a `{message, line?}` object on a Lua runtime error. */
export interface WasmCoreLike {
  setSource(src: string): unknown;
  frame(t: number, f: number): void;
  framebuffer(): ArrayLike<number>;
  registers(): unknown;
  cgram(): Uint16Array;
  oam?: () => OamSprite[];
  listAssets?: () => AssetInfo[];
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
    frame(t: number, f: number): FrameResult {
      core.frame(t, f); // throws on Lua runtime error -> transport.safeFrame surfaces it
      return {
        framebuffer: new Uint8ClampedArray(core.framebuffer()),
        registers: core.registers() as RegisterView[],
        cgram: core.cgram(),
        oam: core.oam?.() ?? [],
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
  };
}

/** Load the wasm-pack module and adapt it to the PpuCore interface. */
export async function createWasmPpuCore(): Promise<PpuCore> {
  await init();
  return wrapWasmCore(new WasmCore() as unknown as WasmCoreLike);
}
