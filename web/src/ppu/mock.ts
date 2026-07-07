import {
  PpuCore,
  FrameResult,
  RegisterView,
  OamSprite,
  AssetInfo,
  ImportReport,
  WIDTH,
  HEIGHT,
} from "./core";

/** Animated placeholder PpuCore for the UI track. Runs no Lua — it synthesizes
 *  a time-varying framebuffer, registers, and CGRAM so the Studio visibly moves
 *  while the real WASM core is built in parallel. Implements PpuCore (core.ts). */
export class MockPpuCore implements PpuCore {
  private assets = new Map<string, ImageData>();
  /** Layer id -> visible. Absent key means visible (default on). */
  private layerVisible = new Map<string, boolean>();
  /** Last frame's register values by addr, for change detection. */
  private prevReg = new Map<number, number>();

  private visible(id: string): boolean {
    return this.layerVisible.get(id) !== false;
  }

  setSource(_src: string) {
    return { ok: true };
  }

  setLayerVisible(id: string, visible: boolean) {
    this.layerVisible.set(id, visible);
  }

  uploadTexture(slot: string, imageData: ImageData) {
    this.assets.set(slot, imageData);
  }

  listAssets(): AssetInfo[] {
    return Array.from(this.assets, ([id, img]) => ({
      id,
      width: img.width,
      height: img.height,
    }));
  }

  vram(): Uint16Array {
    return new Uint16Array(0x8000);
  }

  importReports(): ImportReport[] {
    if (this.assets.size === 0) return [];
    return [
      {
        mode: "tile",
        layer: 0,
        report: {
          colors_used: 8,
          palettes_used: 1,
          tile_cells: 4,
          unique_tiles: 4,
          vram_words: 68,
          overflows: [],
        },
      },
    ];
  }

  frame(t: number, f: number): FrameResult {
    const framebuffer = new Uint8ClampedArray(WIDTH * HEIGHT * 4);
    const bgOn = this.visible("bg1");
    const objOn = this.visible("obj");
    const scroll = t * 32; // px/sec the bands drift
    const tint = this.assets.size > 0 ? 40 : 0; // uploads nudge the output

    // background: scrolling diagonal colour bands (layer "bg1")
    if (bgOn) {
      for (let y = 0; y < HEIGHT; y++) {
        for (let x = 0; x < WIDTH; x++) {
          const i = (y * WIDTH + x) * 4;
          framebuffer[i] = 32 + 32 * Math.sin((y + scroll) * 0.04);
          framebuffer[i + 1] = 48 + 48 * Math.sin((x + y + scroll) * 0.05);
          framebuffer[i + 2] = 96 + 64 * Math.cos((x - scroll) * 0.03) + tint;
          framebuffer[i + 3] = 255;
        }
      }
    } else {
      for (let y = 0; y < HEIGHT; y++) {
        for (let x = 0; x < WIDTH; x++) framebuffer[(y * WIDTH + x) * 4 + 3] = 255;
      }
    }

    // a moving sprite-like blob (layer "obj")
    if (objOn) {
      const cx = WIDTH / 2 + Math.sin(t * 2) * 80;
      const cy = HEIGHT / 2 + Math.cos(t * 1.5) * 60;
      const rad = 14;
      const x0 = Math.max(0, Math.floor(cx - rad));
      const x1 = Math.min(WIDTH - 1, Math.ceil(cx + rad));
      const y0 = Math.max(0, Math.floor(cy - rad));
      const y1 = Math.min(HEIGHT - 1, Math.ceil(cy + rad));
      for (let y = y0; y <= y1; y++) {
        for (let x = x0; x <= x1; x++) {
          const dx = x - cx;
          const dy = y - cy;
          if (dx * dx + dy * dy <= rad * rad) {
            const i = (y * WIDTH + x) * 4;
            framebuffer[i] = 255;
            framebuffer[i + 1] = 200;
            framebuffer[i + 2] = 0;
            framebuffer[i + 3] = 255;
          }
        }
      }
    }

    // registers: values move with t/f; `changed` flags differences vs last frame
    const raw = [
      { addr: 0x2100, name: "INIDISP", value: 0x0f },
      { addr: 0x2105, name: "BGMODE", value: 0x01 },
      { addr: 0x210d, name: "BG1HOFS", value: Math.floor(scroll) & 0xff },
      { addr: 0x210e, name: "BG1VOFS", value: Math.floor(t * 16) & 0xff },
      { addr: 0x2132, name: "COLDATA", value: f & 0xff },
    ];
    const registers: RegisterView[] = raw.map((r) => {
      const prev = this.prevReg.get(r.addr);
      this.prevReg.set(r.addr, r.value);
      return { ...r, changed: prev !== undefined && prev !== r.value };
    });

    // cgram: base gradient + a colour-cycling palette window at 0x40..0x4f
    const cgram = new Uint16Array(256);
    for (let i = 0; i < 256; i++) cgram[i] = (i * 0x84) & 0x7fff;
    for (let i = 0; i < 16; i++) {
      cgram[0x40 + i] = hslTo15((t * 90 + i * 24) % 360, 0.7, 0.5);
    }

    // oam: 24 active sprites orbiting the center; rest are off. Honors obj layer.
    const oam: OamSprite[] = [];
    for (let i = 0; i < 128; i++) {
      const baseOn = i < 24;
      const ang = t * 1.5 + i * 0.5;
      oam.push({
        x: baseOn ? Math.round(WIDTH / 2 + Math.cos(ang) * 90) & 0x1ff : 0,
        y: baseOn ? Math.round(HEIGHT / 2 + Math.sin(ang) * 70) & 0xff : 0,
        tile: (i + (f >> 3)) & 0xff,
        pal: i % 8,
        prio: i % 4,
        size: i % 2,
        flipX: ((f >> 4) & 1) === 1 && i % 3 === 0,
        flipY: false,
        on: baseOn && objOn,
      });
    }

    return { framebuffer, registers, cgram, oam };
  }
}

/** HSL -> packed 15-bit BGR (5 bits each) — the SNES CGRAM colour format. */
function hslTo15(h: number, s: number, l: number): number {
  const c = (1 - Math.abs(2 * l - 1)) * s;
  const hp = h / 60;
  const x = c * (1 - Math.abs((hp % 2) - 1));
  let r = 0;
  let g = 0;
  let b = 0;
  if (hp < 1) [r, g, b] = [c, x, 0];
  else if (hp < 2) [r, g, b] = [x, c, 0];
  else if (hp < 3) [r, g, b] = [0, c, x];
  else if (hp < 4) [r, g, b] = [0, x, c];
  else if (hp < 5) [r, g, b] = [x, 0, c];
  else [r, g, b] = [c, 0, x];
  const m = l - c / 2;
  const q = (v: number) => Math.round((v + m) * 31) & 0x1f;
  return (q(b) << 10) | (q(g) << 5) | q(r);
}
