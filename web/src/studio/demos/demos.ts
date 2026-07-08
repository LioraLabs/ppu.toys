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

// ── procedural sources ───────────────────────────────────────────────────────
// Authored at full screen size (256x224) so the BG layers fill the frame and do
// NOT tile vertically. A SNES BG plane wraps a source smaller than the screen, so
// a 64x64 sky would repeat ~3.5x down the frame (banding). The Rust engine's
// vertical wrap is exercised independently by crates/ppu-core/tests/golden_demos.rs;
// these are tuned for how the flagship demo looks, not byte-identity with it.
const SCREEN_W = 256, SCREEN_H = 224;
const HORIZON = 140; // sky opaque above; transparent below so hills (bg2) show

function sky(): DemoAsset {
  const w = SCREEN_W, h = SCREEN_H;
  const data = new Uint8ClampedArray(w * h * 4);
  const sunX = 192, sunY = 50, sunR = 20;
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const i = (y * w + x) * 4;
      if (y >= HORIZON) {
        data[i + 3] = 0; // below the horizon -> transparent, hills show through
        continue;
      }
      const dx = x - sunX, dy = y - sunY;
      if (dx * dx + dy * dy < sunR * sunR) {
        data[i] = 255; data[i + 1] = 226; data[i + 2] = 168; data[i + 3] = 255; // sun
        continue;
      }
      // dusk vertical gradient: deep indigo up top -> warm pink at the horizon
      const t = y / HORIZON;
      data[i] = 30 + Math.round(t * t * 210);
      data[i + 1] = 18 + Math.round(t * 70);
      data[i + 2] = 78 + Math.round(t * 52);
      data[i + 3] = 255;
    }
  }
  return { id: "sky", width: w, height: h, data };
}

function hills(): DemoAsset {
  const w = SCREEN_W, h = SCREEN_H;
  const data = new Uint8ClampedArray(w * h * 4);
  const top = HORIZON - 2; // slight overlap (hidden behind sky) avoids a seam
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const i = (y * w + x) * 4;
      if (y < top) {
        data[i + 3] = 0; // above the ground -> transparent (sky shows)
        continue;
      }
      const stripe = Math.floor(x / 16) % 2; // vertical bands make scroll visible
      const d = (y - top) / (h - top); // 0 at ground line -> 1 at the bottom
      data[i] = 18 + stripe * 10;
      data[i + 1] = 96 - Math.round(d * 46) + stripe * 12;
      data[i + 2] = 38 + stripe * 8;
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
  const w = 1024, h = 1024;
  const data = new Uint8ClampedArray(w * h * 4);
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const cx = Math.floor(x / 8) % 8;
      const cy = Math.floor(y / 8) % 8;
      const i = (y * w + x) * 4;
      data[i] = cx * 32;
      data[i + 1] = cy * 32;
      data[i + 2] = ((cx + cy) & 1) * 255;
      data[i + 3] = 255;
    }
  }
  return { id: "track", width: w, height: h, data };
}

function ribbons(): DemoAsset {
  const w = SCREEN_W, h = SCREEN_H;
  const data = new Uint8ClampedArray(w * h * 4);
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const i = (y * w + x) * 4;
      const band = Math.floor(x / 8) % 8;
      data[i] = 32 + band * 24;
      data[i + 1] = 40 + (Math.floor(y / 8) % 8) * 24;
      data[i + 2] = 220 - band * 16;
      data[i + 3] = 255;
    }
  }
  return { id: "ribbons", width: w, height: h, data };
}

// ── Lua sources (verbatim from golden_demos.rs DUSK_SRC / MODE7_SRC) ──────────
const DUSK_SRC = `-- ppu.toys :: dusk-parallax (Mode 1: parallax BG scroll + CGRAM colour-cycle + sprite)
local SPEED = 12
function frame(t, f)
  mode = 1; brightness = 15
  bg[1].source = "sky";   bg[2].source = "hills"
  bg[2].map_base = 0x0800; bg[2].char_base = 0x4000
  bg[1].scroll.x = t * SPEED
  bg[2].scroll.x = t * SPEED * 3
  for i = 0, 7 do cgram[0x40 + i] = hsl((t*40 + i*12) % 360, 0.6, 0.5) end
  obj[0].tile = 4; obj[0].pal = 0; obj[0].prio = 3; obj[0].x = 120; obj[0].y = 132 + sin(t*3) * 4
  obj.char_base = 0x6000; obj.sheet = "hero"; obj[0].on = true
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

const OFFSET_SRC = `-- ppu.toys :: offset-per-tile (Mode 2: BG3 table drives per-column scroll)
function column_offset(col, dh, dv)
  local base = 0x0800
  bg[3].map_base = base
  local enable = 0x2000
  vram[base + col] = enable + (dh % 1024)
  vram[base + 32 + col] = enable + 0x8000 + (dv % 1024)
end

function frame(t, f)
  mode = 2; brightness = 15
  bg[1].source = "ribbons"
  bg[1].char_base = 0x1000
  bg[3].map_base = 0x0800
  for col = 0, 31 do
    local wave = floor((sin((col + t * 8) / 3) + 1) * 4)
    column_offset(col, wave, col % 3)
  end
end
`;

export const DEMOS: Demo[] = [
  { id: "dusk-parallax", label: "dusk-parallax", source: DUSK_SRC, assets: [sky(), hills(), hero()] },
  { id: "mode7-floor", label: "mode7-floor", source: MODE7_SRC, assets: [track()] },
  { id: "offset-per-tile", label: "offset-per-tile", source: OFFSET_SRC, assets: [ribbons()] },
];
