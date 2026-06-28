import init, { PpuCore as WasmCore } from "../wasm/pkg/ppu_core.js";
import { PpuCore, FrameResult, RegisterView, LuaError } from "./core";

/** Load the wasm-pack module and adapt it to the PpuCore interface. The Rust
 *  side returns framebuffer/cgram as typed arrays and registers via serde; this
 *  wrapper assembles frame()'s result object. */
export async function createWasmPpuCore(): Promise<PpuCore> {
  await init();
  const core = new WasmCore();
  return {
    setSource(src: string) {
      return core.setSource(src) as { ok: boolean; error?: LuaError };
    },
    frame(t: number, f: number): FrameResult {
      core.frame(t, f);
      return {
        framebuffer: new Uint8ClampedArray(core.framebuffer()),
        registers: core.registers() as RegisterView[],
        cgram: core.cgram(),
      };
    },
    uploadTexture(slot: string, imageData: ImageData) {
      core.uploadTexture(slot, imageData);
    },
    setLayerVisible(id: string, visible: boolean) {
      core.setLayerVisible(id, visible);
    },
  };
}
