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

export interface OamSprite {
  x: number;
  y: number;
  tile: number;
  pal: number; // 0..7
  prio: number; // 0..3
  size: number; // 0 = small, 1 = large
  flipX: boolean;
  flipY: boolean;
  on: boolean;
}

export interface AssetInfo {
  id: string;
  width: number;
  height: number;
}

export interface FrameResult {
  framebuffer: Uint8ClampedArray; // 256*224*4 RGBA
  registers: RegisterView[];
  cgram: Uint16Array;
  oam: OamSprite[]; // 128 sprite entries -> SPRITES inspector
}

/** The one hard seam. Headless — no canvas. Both the mock and the real WASM
 *  module implement this; JS owns presentation. */
export interface PpuCore {
  setSource(src: string): { ok: boolean; error?: LuaError };
  frame(t: number, f: number): FrameResult;
  uploadTexture(slot: string, imageData: ImageData): void;
  setLayerVisible(id: string, visible: boolean): void;
  /** Enumerate uploaded sources resident in VRAM -> VRAM inspector. */
  listAssets(): AssetInfo[];
}

export const WIDTH = 256;
export const HEIGHT = 224;
