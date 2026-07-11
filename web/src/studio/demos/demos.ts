/** Bundled flagship demos: Lua source + the procedural image sources they need,
 *  so each renders immediately without the user drag-dropping files. Pure +
 *  node-safe (no DOM): assets are raw RGBA, wrapped into ImageData by loadDemo.
 *  Pixel generators mirror crates/ppu-core/tests/golden_demos.rs (the Lua sources
 *  ARE verbatim; some assets are retuned for on-screen looks — see below). */
import { EMPTY_POKES } from "../pokes/pokes";

export interface DemoAsset {
  /** Literal slot id referenced from Lua (bg[n].source / obj.sheet). */
  id: string;
  width: number;
  height: number;
  data: Uint8ClampedArray; // width*height*4 RGBA
}

export interface DemoFile {
  name: string;
  source: string;
}

export interface Demo {
  id: string;
  label: string;
  /** Single-file form. For multi-file demos this is the files joined in tab
   *  order with "\n" — the concatenation the parity golden proves equivalent. */
  source: string;
  /** Multi-file demos only. Tab order = chunk execution order (PICO-8 scope). */
  files?: DemoFile[];
  assets: DemoAsset[];
}

/** Ordered files of a demo — single-file demos present as one main.lua. */
export function demoFiles(d: Demo): DemoFile[] {
  return d.files ?? [{ name: "main.lua", source: d.source }];
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

function panel(): DemoAsset {
  const w = SCREEN_W, h = SCREEN_H;
  const data = new Uint8ClampedArray(w * h * 4);
  for (let y = 0; y < h; y++) {
    const opaque = y >= 80 && y < 160;
    for (let x = 0; x < w; x++) {
      const i = (y * w + x) * 4;
      if (opaque) { data[i] = 80; data[i + 1] = 230; data[i + 2] = 255; data[i + 3] = 255; }
    }
  }
  return { id: "panel", width: w, height: h, data };
}

function gradient(): DemoAsset {
  const w = SCREEN_W, h = SCREEN_H;
  const data = new Uint8ClampedArray(w * h * 4);
  for (let y = 0; y < h; y++) {
    // top->bottom hue sweep, constant across x (matches golden_demos.rs gradient()).
    const r = Math.floor((y * 255) / (h - 1));
    const g = Math.floor(((h - 1 - y) * 255) / (h - 1));
    for (let x = 0; x < w; x++) {
      const i = (y * w + x) * 4;
      data[i] = r;
      data[i + 1] = g;
      data[i + 2] = 128;
      data[i + 3] = 255;
    }
  }
  return { id: "gradient", width: w, height: h, data };
}

function ramp(): DemoAsset {
  // 32px-period sawtooth in x AND y (mirrors golden_demos.rs ramp()): fine sub-8px
  // detail that mosaic flattens into flat blocks. Only 16 unique 8x8 tiles.
  const w = SCREEN_W, h = SCREEN_H;
  const data = new Uint8ClampedArray(w * h * 4);
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const i = (y * w + x) * 4;
      data[i] = (x % 32) * 8;
      data[i + 1] = (y % 32) * 8;
      data[i + 2] = 128;
      data[i + 3] = 255;
    }
  }
  return { id: "ramp", width: w, height: h, data };
}

// ── Lua sources (verbatim from golden_demos.rs DUSK_MAIN_SRC / DUSK_PALETTE_SRC / MODE7_SRC) ──
const DUSK_MAIN_SRC = `-- ppu.toys :: dusk-parallax (Mode 1: parallax BG scroll + CGRAM colour-cycle + sprite)
-- Multi-file flagship: SPEED + dusk_palette() live in palette.lua. Chunks run in
-- tab order into ONE shared global scope; frame() resolves after all chunks, so
-- main.lua may reference palette.lua globals freely (main.lua is convention, not magic).
function frame(t, f)
  apply_pokes()
  mode = 1; brightness = 15
  bg[1].source = "sky";   bg[2].source = "hills"
  bg[2].map_base = 0x0800; bg[2].char_base = 0x4000
  bg[1].scroll.x = t * SPEED
  bg[2].scroll.x = t * SPEED * 3
  dusk_palette(t)
  obj[0].tile = 4; obj[0].pal = 0; obj[0].prio = 3; obj[0].x = 120; obj[0].y = 132 + sin(t*3) * 4
  obj.char_base = 0x6000; obj.sheet = "hero"; obj[0].on = true
end
`;

const DUSK_PALETTE_SRC = `-- dusk-parallax :: palette.lua — CGRAM colour-cycle ($40-$47), globals shared with main.lua
SPEED = 12
function dusk_palette(t)
  for i = 0, 7 do cgram[0x40 + i] = hsl((t*40 + i*12) % 360, 0.6, 0.5) end
end
`;

const MODE7_SRC = `-- ppu.toys :: mode7-floor (the namesake; per-scanline affine floor)
function frame(t, f)
  apply_pokes()
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
  apply_pokes()
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

const MODE3_SRC = `-- ppu.toys :: mode3-gradient (Mode 3: 8bpp 256-colour BG1 gradient)
function frame(t, f)
  apply_pokes()
  mode = 3; brightness = 15
  bg[1].source = "gradient"
  bg[1].char_base = 0x1000
end
`;

const TRANSLUCENCY_SRC = `-- ppu.toys :: translucency (½-add glass panel over a scrolling BG)
function frame(t, f)
  apply_pokes()
  mode = 1; brightness = 15
  bg[1].source = "panel"                       -- the glass panel (main only)
  bg[2].source = "ribbons"; bg[2].char_base = 0x2000  -- scene, on main AND sub
  bg[2].map_base = 0x0800
  screen.main.bg1 = true; screen.main.bg2 = true      -- panel + scene on the main screen
  screen.main.bg3 = false; screen.main.bg4 = false; screen.main.obj = false  -- power-on defaults ALL layers on: drop the rest
  screen.sub.bg2 = true    -- scene on the sub screen -> the addend under the glass
  color.op = "add"; color.half = true; color.on.bg1 = true  -- ½-add math on BG1 (the glass)
  color.addend = "sub"     -- addend = subscreen (not fixed colour)
end
`;

const SPOTLIGHT_SRC = `-- ppu.toys :: spotlight (per-scanline circular iris via the colour window)
function frame(t, f)
  apply_pokes()
  mode = 1; brightness = 15
  bg[1].source = "ribbons"
  screen.main.bg1 = true    -- BG1 only on the main screen
  screen.main.bg2 = false; screen.main.bg3 = false   -- power-on defaults ALL layers on: drop the rest
  screen.main.bg4 = false; screen.main.obj = false
  win.color.w1 = true       -- COLOR window follows window 1
  win.color.combine = "OR"  -- COLOR window logic = OR
  -- clip-to-black = 01 (outside the window -> black); raw on purpose: CGWSEL
  -- bits 6-7 have no friendly field (color owns only addend/region)
  CGWSEL = 0x40
  -- iris: per scanline, window 1 spans [cx-hw, cx+hw] where hw traces a circle.
  local cx, cy, r = 128, 112, 70
  hdma(0, 223, function(y)
    local dy = y - cy
    local inside = r*r - dy*dy
    if inside < 0 then
      win.w1.lo = 1; win.w1.hi = 0   -- empty span (left > right) -> nothing inside
    else
      local hw = floor(sqrt(inside))
      win.w1.lo = cx - hw
      win.w1.hi = cx + hw
    end
  end)
end
`;

const GLOW_SRC = `-- ppu.toys :: additive-glow (fixed-colour add brightens BG1 toward warm)
function frame(t, f)
  apply_pokes()
  mode = 1; brightness = 15
  bg[1].source = "ribbons"
  screen.main.bg1 = true    -- BG1 only on the main screen
  screen.main.bg2 = false; screen.main.bg3 = false   -- power-on defaults ALL layers on: drop the rest
  screen.main.bg4 = false; screen.main.obj = false
  color.op = "add"; color.on.bg1 = true   -- add at full strength (half stays off)
  color.addend = "fixed"    -- addend = the fixed colour, not the sub screen
  color.fixed = rgb(120, 60, 0)  -- warm glow added to every BG1 pixel
end
`;

const SPRITE_STORM_SRC = `-- ppu.toys :: sprite-storm (authentic OBJ flicker: >32 sprites on one band, OAM start rotates each frame)
function frame(t, f)
  apply_pokes()
  mode = 1; brightness = 15
  obj.char_base = 0x4000
  obj.size_sel = 7           -- small 16x32 (non-square), large 32x32
  -- solid 4bpp OBJ tiles (index 1) so large sprites fill fully
  for tn = 0, 63 do
    local base = 0x4000 + tn * 16
    for y = 0, 7 do vram[base + y] = 0x00ff end
  end
  cgram[0] = rgb(24, 16, 48)               -- backdrop
  for p = 0, 7 do cgram[128 + p * 16 + 1] = hsl(p * 44, 0.8, 0.55) end
  local N = 48
  for i = 0, N - 1 do
    obj[i].tile = 0; obj[i].pal = i % 8
    obj[i].x = 8 + (i * 15) % 232; obj[i].y = 96
    obj[i].large = (i % 12 == 0)           -- a few 32x32 among the 16x32 storm
    obj[i].on = true
  end
  obj.first = f % N                        -- rotate OAM eval start -> flicker
end
`;

const MOSAIC_SRC = `-- ppu.toys :: mosaic (BG1 pixelation; block size steps every 8 frames)
function frame(t, f)
  apply_pokes()
  mode = 3; brightness = 15
  bg[1].source = "ramp"
  bg[1].mosaic = true
  mosaic = floor(f / 8) % 16
end
`;

const EXTBG_SRC = `-- ppu.toys :: mode7-extbg (per-pixel floor priority; sprite between the two levels)
function frame(t, f)
  apply_pokes()
  mode = 7; brightness = 15
  m7.a, m7.d = 1, 1
  m7.extbg = true
  cgram[1] = rgb(216, 64, 64)          -- Mode 7 floor colour 1 = red
  cgram[128 + 1] = rgb(255, 255, 0)    -- OBJ pal0 idx1 = yellow
  for fy = 0, 7 do
    for fx = 0, 7 do
      m7pixel(1, fx, fy, 0x81)         -- high priority (bit7) + colour 1
      m7pixel(2, fx, fy, 0x01)         -- low priority + colour 1
    end
  end
  for ty = 0, 27 do
    m7.map[ty] = {}
    for tx = 0, 31 do m7.map[ty][tx] = (tx < 16) and 1 or 2 end
  end
  obj.char_base = 0x4000
  obj.size_sel = 1                     -- large pair = 32x32
  for row = 0, 3 do                    -- fill the 4x4 tile block solid (index 1)
    for col = 0, 3 do
      local base = 0x4000 + (row * 16 + col) * 16
      for y = 0, 7 do vram[base + y] = 0x00ff end
    end
  end
  obj[0].tile = 0; obj[0].pal = 0; obj[0].prio = 2
  obj[0].large = true                  -- 32x32
  obj[0].x = 112; obj[0].y = 88; obj[0].on = true
end
`;

const DIRECT_SRC = `-- ppu.toys :: direct-color (8bpp Mode 7, CGRAM bypass, smooth colour field)
function frame(t, f)
  apply_pokes()
  mode = 7; brightness = 15
  m7.a, m7.d = 1, 1
  direct_color = true
  local done = {}
  for ty = 0, 27 do
    m7.map[ty] = {}
    for tx = 0, 31 do
      local r = floor(tx * 7 / 31)
      local g = floor(ty * 7 / 27)
      local b = 1 + floor((tx + ty) * 2 / 58)
      local idx = r + g * 8 + b * 64
      m7.map[ty][tx] = idx
      if not done[idx] then
        done[idx] = true
        for fy = 0, 7 do for fx = 0, 7 do m7pixel(idx, fx, fy, idx) end end
      end
    end
  end
end
`;

// ── demo assembly: every demo ships a generated, read-only pokes.lua first ──
// (main.lua's frame() calls apply_pokes() as its first line, matching what
// openSketch/newSketch already do for user sketches — see pokes/pokes.ts).

/** Files joined in tab order with "\n" — the Demo.source doc contract above. */
function demoSource(files: DemoFile[]): string {
  return files.map((f) => f.source).join("\n");
}

/** Build a Demo from its non-pokes files: prepends the generated pokes.lua
 *  and derives `source` from the full (pokes-included) file list. */
function demo(id: string, label: string, files: DemoFile[], assets: DemoAsset[]): Demo {
  const withPokes = [{ name: "pokes.lua", source: EMPTY_POKES }, ...files];
  return { id, label, source: demoSource(withPokes), files: withPokes, assets };
}

export const DEMOS: Demo[] = [
  demo(
    "dusk-parallax",
    "dusk-parallax",
    [
      { name: "main.lua", source: DUSK_MAIN_SRC },
      { name: "palette.lua", source: DUSK_PALETTE_SRC },
    ],
    [sky(), hills(), hero()],
  ),
  demo("mode7-floor", "mode7-floor", [{ name: "main.lua", source: MODE7_SRC }], [track()]),
  demo("offset-per-tile", "offset-per-tile", [{ name: "main.lua", source: OFFSET_SRC }], [ribbons()]),
  demo("mode3-gradient", "mode3-gradient", [{ name: "main.lua", source: MODE3_SRC }], [gradient()]),
  demo(
    "translucency",
    "translucency",
    [{ name: "main.lua", source: TRANSLUCENCY_SRC }],
    [panel(), ribbons()],
  ),
  demo("spotlight", "spotlight", [{ name: "main.lua", source: SPOTLIGHT_SRC }], [ribbons()]),
  demo("glow", "glow", [{ name: "main.lua", source: GLOW_SRC }], [ribbons()]),
  demo("sprite-storm", "sprite-storm", [{ name: "main.lua", source: SPRITE_STORM_SRC }], []),
  demo("mosaic", "mosaic", [{ name: "main.lua", source: MOSAIC_SRC }], [ramp()]),
  demo("mode7-extbg", "mode7-extbg", [{ name: "main.lua", source: EXTBG_SRC }], []),
  demo("direct-color", "direct-color", [{ name: "main.lua", source: DIRECT_SRC }], []),
];
