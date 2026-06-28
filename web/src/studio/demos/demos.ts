/** Bundled flagship demos: Lua source + the procedural image sources they need,
 *  so each renders immediately without the user drag-dropping files. Pure +
 *  node-safe (no DOM): assets are raw RGBA, wrapped into ImageData by loadDemo.
 *  Pixel generators reproduce crates/ppu-core/tests/golden_demos.rs byte-for-byte
 *  so the live WASM output matches the proven golden fixtures. */
export interface DemoAsset {
  /** Literal slot id referenced from Lua (bg[n].source / obj.sheet). */
  id: string;
  width: number;
  height: number;
  data: Uint8ClampedArray; // width*height*4 RGBA
}

export interface Demo {
  id: string;
  label: string;
  source: string;
  assets: DemoAsset[];
}

// ── procedural sources (mirror golden_demos.rs) ──────────────────────────────
function sky(): DemoAsset {
  const w = 64, h = 64;
  const data = new Uint8ClampedArray(w * h * 4);
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const i = (y * w + x) * 4;
      if (y >= h / 2) {
        data[i + 3] = 0; // transparent lower half -> hills shows through
        continue;
      }
      const stripe = Math.floor((x + y) / 4) % 2;
      data[i] = 80 + stripe * 60;
      data[i + 1] = 40 + y;
      data[i + 2] = 120 + stripe * 40;
      data[i + 3] = 255;
    }
  }
  return { id: "sky", width: w, height: h, data };
}

function hills(): DemoAsset {
  const w = 64, h = 64;
  const data = new Uint8ClampedArray(w * h * 4);
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const i = (y * w + x) * 4;
      const band = Math.floor(x / 8);
      data[i] = 20 + band * 16;
      data[i + 1] = 60 + band * 20;
      data[i + 2] = 30;
      data[i + 3] = 255;
    }
  }
  return { id: "hills", width: w, height: h, data };
}

function hero(): DemoAsset {
  const w = 64, h = 8;
  const data = new Uint8ClampedArray(w * h * 4);
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const i = (y * w + x) * 4;
      const cell = Math.floor(x / 8);
      data[i] = 255 - cell * 16;
      data[i + 1] = 200;
      data[i + 2] = cell * 24;
      data[i + 3] = 255;
    }
  }
  return { id: "hero", width: w, height: h, data };
}

function track(): DemoAsset {
  const w = 64, h = 64;
  const data = new Uint8ClampedArray(w * h * 4);
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const cx = Math.floor(x / 8);
      const cy = Math.floor(y / 8);
      const i = (y * w + x) * 4;
      data[i] = cx * 32;
      data[i + 1] = cy * 32;
      data[i + 2] = ((cx + cy) & 1) * 255;
      data[i + 3] = 255;
    }
  }
  return { id: "track", width: w, height: h, data };
}

// ── Lua sources (verbatim from golden_demos.rs DUSK_SRC / MODE7_SRC) ──────────
const DUSK_SRC = `-- ppu.toys :: dusk-parallax (Mode 1: parallax BG scroll + CGRAM colour-cycle + sprite)
local SPEED = 12
function frame(t, f)
  mode = 1; brightness = 15
  bg[1].source = "sky";   bg[2].source = "hills"
  bg[1].scroll.x = t * SPEED
  bg[2].scroll.x = t * SPEED * 3
  for i = 0, 7 do cgram[0x40 + i] = hsl((t*40 + i*12) % 360, 0.6, 0.5) end
  obj[0].tile = 4; obj[0].pal = 2; obj[0].x = 120; obj[0].y = 132 + sin(t*3) * 4
  obj.sheet = "hero"; obj[0].on = true
end
`;

const MODE7_SRC = `-- ppu.toys :: mode7-floor (the namesake; per-scanline affine floor)
function frame(t, f)
  mode = 7; brightness = 15; bg[1].source = "track"
  hdma(96, 223, function(y)
    local d = 64 / (y - 95)
    m7.a, m7.d = d, d
    m7.cx, m7.cy = 128, 0
    bg[1].scroll.y = (t*80) * d
  end)
end
`;

export const DEMOS: Demo[] = [
  { id: "dusk-parallax", label: "dusk-parallax", source: DUSK_SRC, assets: [sky(), hills(), hero()] },
  { id: "mode7-floor", label: "mode7-floor", source: MODE7_SRC, assets: [track()] },
];
