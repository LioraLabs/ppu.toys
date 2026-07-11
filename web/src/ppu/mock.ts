import {
  PpuCore,
  FrameResult,
  RegisterView,
  OamSprite,
  ObjOverflow,
  AssetInfo,
  ImportReport,
  SourceFile,
  WIDTH,
  HEIGHT,
  CompositorScreens,
  PlaneId,
  BgTrace,
  ObjTrace,
  SourceKind,
  ConvertSourceOptions,
  ConvertSourceResult,
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
  private lastT = 0;
  private lastF = 0;
  /** frame() output cached for the side queries (screens/traceObj). */
  private lastFrame: FrameResult | null = null;

  private visible(id: string): boolean {
    return this.layerVisible.get(id) !== false;
  }

  setSource(src: string) {
    return this.setSources([{ name: "main.lua", source: src }]);
  }

  setSources(_files: SourceFile[]) {
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

  /** background: scrolling diagonal colour bands (layer "bg1") */
  private drawBands(fb: Uint8ClampedArray, t: number, tint: number) {
    const scroll = t * 32; // px/sec the bands drift
    for (let y = 0; y < HEIGHT; y++) {
      for (let x = 0; x < WIDTH; x++) {
        const i = (y * WIDTH + x) * 4;
        fb[i] = 32 + 32 * Math.sin((y + scroll) * 0.04);
        fb[i + 1] = 48 + 48 * Math.sin((x + y + scroll) * 0.05);
        fb[i + 2] = 96 + 64 * Math.cos((x - scroll) * 0.03) + tint;
        fb[i + 3] = 255;
      }
    }
  }

  /** a moving sprite-like blob (layer "obj") */
  private drawBlob(fb: Uint8ClampedArray, t: number) {
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
          fb[i] = 255;
          fb[i + 1] = 200;
          fb[i + 2] = 0;
          fb[i + 3] = 255;
        }
      }
    }
  }

  frame(t: number, f: number): FrameResult {
    this.lastT = t;
    this.lastF = f;
    const framebuffer = new Uint8ClampedArray(WIDTH * HEIGHT * 4);
    const bgOn = this.visible("bg1");
    const objOn = this.visible("obj");
    const scroll = t * 32; // px/sec the bands drift
    const tint = this.assets.size > 0 ? 40 : 0; // uploads nudge the output

    if (bgOn) {
      this.drawBands(framebuffer, t, tint);
    } else {
      for (let y = 0; y < HEIGHT; y++) {
        for (let x = 0; x < WIDTH; x++) framebuffer[(y * WIDTH + x) * 4 + 3] = 255;
      }
    }

    if (objOn) this.drawBlob(framebuffer, t);

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
        large: i % 2 === 1,
        flipX: ((f >> 4) & 1) === 1 && i % 3 === 0,
        flipY: false,
        on: baseOn && objOn,
      });
    }

    const objOverflow: ObjOverflow = {
      rangeOver: false,
      timeOver: false,
      maxSprites: objOn ? oam.filter((s) => s.on).length : 0,
      maxTiles: objOn ? oam.filter((s) => s.on).length : 0,
    };
    const result: FrameResult = { framebuffer, registers, cgram, oam, objOverflow };
    this.lastFrame = result;
    return result;
  }

  /** Mock intermediates: main = the last synthesized frame, sub = a dimmed
   *  copy, mask = no math anywhere. (Placeholder pixel math is fine here —
   *  this is the mock.) */
  screens(): CompositorScreens {
    const main = (this.lastFrame ?? this.frame(this.lastT, this.lastF)).framebuffer.slice();
    const sub = new Uint8ClampedArray(main.length);
    for (let i = 0; i < main.length; i += 4) {
      sub[i] = main[i] >> 1;
      sub[i + 1] = main[i + 1] >> 1;
      sub[i + 2] = main[i + 2] >> 1;
      sub[i + 3] = 255;
    }
    return { main, sub, mathMask: new Uint8Array(WIDTH * HEIGHT) };
  }

  layerView(plane: PlaneId): Uint8ClampedArray {
    const fb = new Uint8ClampedArray(WIDTH * HEIGHT * 4);
    if (plane === "bg1" && this.visible("bg1")) {
      this.drawBands(fb, this.lastT, this.assets.size > 0 ? 40 : 0);
    }
    if (plane === "obj" && this.visible("obj")) this.drawBlob(fb, this.lastT);
    return fb;
  }

  traceBgPixel(layer: number, x: number, y: number): BgTrace | null {
    const t = this.traceBgTile(layer, x >> 3, y >> 3, y);
    if (!t) return null;
    t.pixel = {
      x,
      y,
      fx: x & 7,
      fy: y & 7,
      index: 1,
      cgramIndex: 1,
      bgr555: 0x7fff,
      rgb: [255, 255, 255],
    };
    return t;
  }

  traceBgTile(layer: number, tx: number, ty: number, _y: number): BgTrace | null {
    if (layer < 1 || layer > 3) return null; // mock reports mode 1: BG4 absent
    return {
      regs: {
        mode: 1,
        layer,
        mapBase: 0,
        charBase: 0x1000,
        tileSize: 8,
        screenSize: 0,
        bpp: layer === 3 ? 2 : 4,
        scrollX: 0,
        scrollY: 0,
        mosaic: 1,
        directColor: false,
        visible: this.visible(`bg${layer}`),
      },
      tile: {
        tx,
        ty,
        mapAddr: (ty % 32) * 32 + (tx % 32),
        entry: 1,
        tile: 1,
        pal: 0,
        prio: false,
        flipX: false,
        flipY: false,
        charAddr: 0x1000 + 16,
        pixels: new Array(64).fill(1),
        paletteBase: 0,
      },
    };
  }

  traceObj(index: number): ObjTrace | null {
    if (index < 0 || index > 127) return null;
    const oam = (this.lastFrame ?? this.frame(this.lastT, this.lastF)).oam[index];
    return {
      index,
      oam,
      charBase: 0x4000,
      charAddr: 0x4000 + oam.tile * 16,
      width: 8,
      height: 8,
      pixels: new Array(64).fill(1),
      paletteBase: 128 + oam.pal * 16,
      palette: Array.from({ length: 16 }, (_, i) => (i * 0x421) & 0x7fff),
    };
  }

  /** The mock renders no real sources — a minimal honest stub. */
  convertSource(_kind: SourceKind, _options: ConvertSourceOptions, _imageData: ImageData): ConvertSourceResult {
    return {
      payload: new Uint8Array([1]),
      meta: {
        width: 0,
        height: 0,
        report: {
          mode: "tile",
          report: { colors_used: 0, palettes_used: 0, tile_cells: 0, unique_tiles: 0, vram_words: 0, overflows: [] },
        },
      },
    };
  }

  addSource(_name: string, _payload: Uint8Array): { ok: boolean; error?: string } {
    return { ok: true };
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
