import { PpuCore, FrameResult, RegisterView, WIDTH, HEIGHT } from "./core";

/** Placeholder PpuCore for the UI track — mirrors the Rust stub's output shape
 *  (ramp framebuffer + a couple of registers + a cgram gradient). */
export class MockPpuCore implements PpuCore {
  setSource(_src: string) {
    return { ok: true };
  }

  frame(t: number, f: number): FrameResult {
    const framebuffer = new Uint8ClampedArray(WIDTH * HEIGHT * 4);
    const b = (Math.floor(t * 60) & 0xff) ^ (f & 0xff);
    for (let y = 0; y < HEIGHT; y++) {
      for (let x = 0; x < WIDTH; x++) {
        const i = (y * WIDTH + x) * 4;
        framebuffer[i] = x;
        framebuffer[i + 1] = y;
        framebuffer[i + 2] = b;
        framebuffer[i + 3] = 255;
      }
    }
    const registers: RegisterView[] = [
      { addr: 0x2100, name: "INIDISP", value: 0x0f, changed: false },
      { addr: 0x2105, name: "BGMODE", value: 0x01, changed: false },
    ];
    const cgram = new Uint16Array(256);
    for (let i = 0; i < 256; i++) cgram[i] = (i * 0x84) & 0x7fff;
    return { framebuffer, registers, cgram };
  }

  uploadTexture(_slot: string, _imageData: ImageData) {}
  setLayerVisible(_id: string, _visible: boolean) {}
}
