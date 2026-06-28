export interface LuaError {
  message: string;
  line?: number;
}

export interface RegisterView {
  addr: number;
  name: string;
  value: number;
  changed: boolean;
}

export interface FrameResult {
  framebuffer: Uint8ClampedArray; // 256*224*4 RGBA
  registers: RegisterView[];
  cgram: Uint16Array;
}

/** The one hard seam. Headless — no canvas. Both the mock and the real WASM
 *  module implement this; JS owns presentation. */
export interface PpuCore {
  setSource(src: string): { ok: boolean; error?: LuaError };
  frame(t: number, f: number): FrameResult;
  uploadTexture(slot: string, imageData: ImageData): void;
  setLayerVisible(id: string, visible: boolean): void;
}

export const WIDTH = 256;
export const HEIGHT = 224;
