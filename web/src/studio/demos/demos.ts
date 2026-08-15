/** Bundled flagship demos: Lua source + the procedural image sources they need,
 *  so each renders immediately without the user drag-dropping files. Pure +
 *  node-safe (no DOM): assets are raw RGBA, wrapped into ImageData by loadDemo.
 *  Pixel generators mirror crates/ppu-core/tests/golden_demos.rs (the Lua sources
 *  ARE verbatim; some assets are retuned for on-screen looks — see below). */
import { EMPTY_POKES } from "../pokes/pokes";
import type { SourceKind, ConvertSourceOptions } from "../../ppu/core";

export interface DemoAsset {
  /** Literal slot id referenced from Lua (bg[n].source / obj.sheet). */
  id: string;
  width: number;
  height: number;
  data: Uint8ClampedArray; // width*height*4 RGBA
  /** Format the generator commits to at bind time (matches the demo's mode). */
  kind: SourceKind;
  options: ConvertSourceOptions;
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
const SCREEN_W = 256,
  SCREEN_H = 224;
const HORIZON = 140; // sky opaque above; transparent below so hills (bg2) show

function sky(): DemoAsset {
  const w = SCREEN_W,
    h = SCREEN_H;
  const data = new Uint8ClampedArray(w * h * 4);
  const sunX = 192,
    sunY = 50,
    sunR = 20;
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const i = (y * w + x) * 4;
      if (y >= HORIZON) {
        data[i + 3] = 0; // below the horizon -> transparent, hills show through
        continue;
      }
      const dx = x - sunX,
        dy = y - sunY;
      if (dx * dx + dy * dy < sunR * sunR) {
        data[i] = 255;
        data[i + 1] = 226;
        data[i + 2] = 168;
        data[i + 3] = 255; // sun
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
  return { id: "sky", width: w, height: h, data, kind: "bg", options: { bit_depth: 4 } };
}

function hills(): DemoAsset {
  const w = SCREEN_W,
    h = SCREEN_H;
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
  return { id: "hills", width: w, height: h, data, kind: "bg", options: { bit_depth: 4 } };
}

function hero(): DemoAsset {
  const w = 64,
    h = 8;
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
  return { id: "hero", width: w, height: h, data, kind: "obj", options: { cell_size: 8 } };
}

function track(): DemoAsset {
  const w = 1024,
    h = 1024;
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
  return { id: "track", width: w, height: h, data, kind: "m7", options: {} };
}

function ribbons(): DemoAsset {
  const w = SCREEN_W,
    h = SCREEN_H;
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
  return { id: "ribbons", width: w, height: h, data, kind: "bg", options: { bit_depth: 4 } };
}

function panel(): DemoAsset {
  const w = SCREEN_W,
    h = SCREEN_H;
  const data = new Uint8ClampedArray(w * h * 4);
  for (let y = 0; y < h; y++) {
    const opaque = y >= 80 && y < 160;
    for (let x = 0; x < w; x++) {
      const i = (y * w + x) * 4;
      if (opaque) {
        data[i] = 80;
        data[i + 1] = 230;
        data[i + 2] = 255;
        data[i + 3] = 255;
      }
    }
  }
  return { id: "panel", width: w, height: h, data, kind: "bg", options: { bit_depth: 4 } };
}

function gradient(): DemoAsset {
  const w = SCREEN_W,
    h = SCREEN_H;
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
  return { id: "gradient", width: w, height: h, data, kind: "bg", options: { bit_depth: 8 } };
}

function ramp(): DemoAsset {
  // 32px-period sawtooth in x AND y (mirrors golden_demos.rs ramp()): fine sub-8px
  // detail that mosaic flattens into flat blocks. Only 16 unique 8x8 tiles.
  const w = SCREEN_W,
    h = SCREEN_H;
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
  return { id: "ramp", width: w, height: h, data, kind: "bg", options: { bit_depth: 8 } };
}

// ── cavern: the tilesheet demo's two sources ────────────────────────────────
// Mirrored byte-for-byte by cavern_tiles()/cavern_back() in
// crates/ppu-core/tests/golden_demos.rs — that mirror is what makes the demo's
// golden PNG evidence about the art the studio actually ships. Edit both.
// A 14-colour master palette shared by BOTH images, on purpose. In mode 1 every
// BG source places its palettes at CGRAM 0 (there is no per-layer CGRAM bank
// outside mode 0's 2bpp layers), and BG2 is placed after BG1 — two different
// palettes would clobber each other.
//
// The invariant that makes the overwrite a no-op has TWO halves, and the second
// is easy to lose: the images must use an identical colour SET, *and* that set
// must fit ONE sub-palette (15 at 4bpp). Sorting alone is not enough — past one
// sub-palette `region_fit` partitions greedily in tile order, so the same colour
// set can split differently between two images and the sorted indices diverge.
// 14 <= 15 is therefore load-bearing, not slack. Channels are multiples of 8
// (the rgb15 grid) so none collapse when 8-bit colour is reduced to 5 — a
// collapse would not break the invariant, but it would waste a slot.
// demos.test.ts pins both halves; golden_demos.rs pins the resulting CGRAM.
const CAVERN_PAL = [
  0x000000, // 0 = transparent, never placed (palette index 0 is the see-through one)
  0x101828, //  1,
  0x283860, //  2,
  0x4870a8, //  3,
  0x78c0e0, //  4,
  0x302820, //  5,
  0x604830, //  6,
  0x987048, //  7,
  0x383840, //  8,
  0x585868, //  9,
  0x909098, // 10,
  0xb83810, // 11,
  0xf07018, // 12,
  0xffc850, // 13,
  0x58a848, // 14
];

/** 24 8x8 cells in row-major SHEET order, so cell N is what `tile = N` draws.
 *  One template literal per cell: 64 hex palette indices, whitespace stripped,
 *  '0' = transparent. Cell 0 is blank deliberately — a sheet has no reserved
 *  blank tile, so the author reserves one (and Tiled's empty gid 0 maps onto it). */
const CAVERN_CELLS = [
  //  0 blank (the author-reserved empty tile: a sheet has no built-in one)
  `
    00000000
    00000000
    00000000
    00000000
    00000000
    00000000
    00000000
    00000000
  `,
  //  1 rock fill
  `
    99a99998
    9a999899
    99989a99
    89999a98
    999a8999
    9998999a
    9a999998
    899a9999
  `,
  //  2 rock top (moss cap)
  `
    eeeeeeee
    9ee9eee9
    a99a99a9
    99a99999
    9899999a
    99998999
    9a999998
    99989a99
  `,
  //  3 rock dark (bedrock)
  `
    88818888
    81888818
    88888188
    88188888
    88888818
    18888188
    88818888
    88888881
  `,
  //  4 dirt fill
  `
    66576665
    66666756
    57666665
    66566766
    66675666
    56666657
    66566666
    66666576
  `,
  //  5 dirt top
  `
    77777777
    67767677
    66676666
    66666576
    56666666
    66657666
    66666665
    65666676
  `,
  //  6 brick course A
  `
    aaaaaaaa
    a999999a
    a999999a
    88888888
    aaaaaaaa
    99a99999
    99999a99
    88888888
  `,
  //  7 brick course B
  `
    9a9988aa
    999988a9
    88888888
    aa999999
    a9999999
    88888888
    9988aa99
    9988a999
  `,
  //  8 lava frame 0
  `
    bbbbbbbb
    bcbbbbcb
    bccbbccb
    cccdcccc
    ccdddccc
    cdddddcc
    dddddddd
    dddddddd
  `,
  //  9 lava frame 1
  `
    bbbbbbbb
    bbcbbcbb
    bcccbccb
    ccccdccc
    cccdddcc
    ccdddddc
    dddddddd
    dddddddd
  `,
  // 10 lava frame 2
  `
    bbbbbbbb
    bbbcbcbb
    bbccbcbb
    cccccdcc
    ccccdddc
    cccddddd
    dddddddd
    dddddddd
  `,
  // 11 lava frame 3
  `
    bbbbbbbb
    cbbbcbbc
    ccbbccbc
    ccccccdc
    cccccddd
    ccccddcd
    dddddddd
    dddddddd
  `,
  // 12 water frame 0
  `
    44433334
    33333333
    33232333
    32222233
    22222222
    22122222
    21112221
    11111111
  `,
  // 13 water frame 1
  `
    34443333
    33333333
    33323233
    33222223
    22222222
    22212222
    12111222
    11111111
  `,
  // 14 water frame 2
  `
    33444333
    33333333
    23332323
    33322222
    22222222
    22221222
    22121112
    11111111
  `,
  // 15 water frame 3
  `
    33344433
    33333333
    32333232
    23332222
    22222222
    22222122
    22212111
    11111111
  `,
  // 16 ledge left
  `
    000aaaaa
    00aa999a
    0aa99999
    aa999998
    a9999899
    a9989999
    89999998
    89999889
  `,
  // 17 ledge mid
  `
    aaaaaaaa
    a99999a9
    999a9999
    99899999
    99998999
    89999998
    99899899
    98999899
  `,
  // 18 ledge right
  `
    aaaaa000
    a999aa00
    99999aa0
    899999aa
    9989999a
    9999899a
    89999998
    98899998
  `,
  // 19 pillar
  `
    0aaaaaa0
    0a9999a0
    0a9889a0
    0a9889a0
    0a9889a0
    0a9889a0
    0a9999a0
    0aaaaaa0
  `,
  // 20 crystal
  `
    00044000
    00434400
    04333440
    04333340
    03333340
    03323330
    00332300
    00033000
  `,
  // 21 pebbles
  `
    00000000
    00090000
    000a9000
    00000000
    0900009a
    0a90000a
    00000000
    0000a900
  `,
  // 22 rock top-left corner
  `
    0000eeee
    000ee99e
    00ee999a
    0ee99999
    ee999a99
    e99999a9
    9998999a
    999a9999
  `,
  // 23 rock top-right corner
  `
    eeee0000
    e99ee000
    a999ee00
    99999ee0
    99a999ee
    9a99999e
    a9998999
    9999a999
  `,
].map((c) => c.replace(/\s/g, ""));

function cavernTiles(): DemoAsset {
  const cols = 8;
  const w = cols * 8,
    h = (CAVERN_CELLS.length / cols) * 8;
  const data = new Uint8ClampedArray(w * h * 4);
  CAVERN_CELLS.forEach((cell, n) => {
    const ox = (n % cols) * 8,
      oy = Math.floor(n / cols) * 8;
    for (let y = 0; y < 8; y++) {
      for (let x = 0; x < 8; x++) {
        const idx = parseInt(cell[y * 8 + x], 16);
        if (!idx) continue; // palette index 0 -> transparent, the backdrop shows
        const c = CAVERN_PAL[idx];
        const i = ((oy + y) * w + ox + x) * 4;
        data[i] = c >> 16;
        data[i + 1] = (c >> 8) & 0xff;
        data[i + 2] = c & 0xff;
        data[i + 3] = 255;
      }
    }
  });
  return {
    id: "cavern_tiles",
    width: w,
    height: h,
    data,
    kind: "sheet",
    options: { bit_depth: 4 },
  };
}

/** Backdrop palette index at (x, y): distant cave pillars in strata bands. The
 *  32px horizontal period makes the 256px plane wrap seamlessly under a plain
 *  `scroll`, and every one of the 14 colours appears — the shared-palette
 *  invariant needs the colour SETS equal, not merely overlapping. */
function cavernBackIndex(x: number, y: number): number {
  const px = x % 32;
  const pillar = px >= 10 && px < 22;
  if (px >= 14 && px < 17 && y >= 70 && y < 73) return 4; // crystal glints on the pillar face
  if (pillar && y >= 126 && y < 129) return 14; // moss cap where the distant rock begins
  if (pillar && (px === 10 || px === 21) && y >= 96 && y < 168) return 10; // lit pillar edge
  if (px >= 15 && px < 18 && y >= 196) return y >= 214 ? 13 : y >= 206 ? 12 : 11; // lava vent
  if (pillar && y >= 188 && y < 196) return 7; // dirt seam catching the vent light
  if (!pillar && y >= 180 && y < 188) return 6;
  // Below the moss line everything recedes into the dark, so a gap in the terrain
  // reads as cave depth rather than as a bright slab.
  if (y < 56) return pillar ? 2 : 1;
  if (y < 96) return pillar ? 3 : 2;
  if (y < 129) return pillar ? 9 : 3;
  if (y < 168) return pillar ? 9 : 8;
  if (y < 200) return pillar ? 8 : 5;
  return pillar ? 5 : 1;
}

function cavernBack(): DemoAsset {
  const w = SCREEN_W,
    h = SCREEN_H;
  const data = new Uint8ClampedArray(w * h * 4);
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const c = CAVERN_PAL[cavernBackIndex(x, y)];
      const i = (y * w + x) * 4;
      data[i] = c >> 16;
      data[i + 1] = (c >> 8) & 0xff;
      data[i + 2] = c & 0xff;
      data[i + 3] = 255;
    }
  }
  return { id: "cavern_back", width: w, height: h, data, kind: "bg", options: { bit_depth: 4 } };
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

const CAVERN_SRC = `-- ppu.toys :: tilesheet-cavern (Mode 1: a camera streaming a Tiled-authored map
-- out of a tilesheet, with animated lava/water, over an assembled-import backdrop)
--
-- The reference implementation of the tilesheet workflow:
--   1. bind a 'sheet' source         -- chars land in sheet order: tile N = cell N
--   2. set the map geometry YOURSELF -- a sheet echoes back only char_base
--   3. rewrite bg[1].map each frame  -- the 64x32 tilemap IS the camera's window
--
-- LEVEL is a Tiled Lua export (File > Export As > Lua) with its 'data' array kept
-- whole, one map row per line as Tiled writes it. To use your own map, replace
-- this table -- and then check the four things below it that are level-specific:
-- ANIM's keys (gids of THIS tileset), MAP_TOP (assumes LEVEL_H rows fit above
-- row 32), pal = 0 in the map write, and the palette constraint on BG2.
local LEVEL = {
  version = "1.10", luaversion = "5.1",
  orientation = "orthogonal", renderorder = "right-down",
  width = 96, height = 12, tilewidth = 8, tileheight = 8,
  tilesets = {
    { name = "cavern_tiles", firstgid = 1, tilewidth = 8, tileheight = 8, tilecount = 24, columns = 8 },
  },
  layers = {
    {
      type = "tilelayer", name = "terrain", x = 0, y = 0,
      width = 96, height = 12, visible = true, opacity = 1, encoding = "lua",
      data = {
         0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,
         0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,
         0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,
         0,  0,  0,  0,  0,  0,  0,  0, 20,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0, 17, 18, 18, 18, 18, 19,  0, 20,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0, 17, 18, 18, 18, 19,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,
         0,  0,  0,  0,  0,  0,  0,  0, 20,  0, 17, 18, 18, 18, 19,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  7,  8,  7,  8,  7, 22,  0, 20,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  7,  8,  7,  8,  7,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,
         0,  0, 22,  0,  0, 21,  0,  0, 20,  0,  7,  8,  7,  8, 22,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0, 24,  3,  3,  3,  3,  3,  3,  3,  3,  0, 20,  0,  0,  0,  0,  0,  0,  0,  0, 17, 18, 18, 19,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0, 17, 18, 18, 19,  0,  0,  0,  0,  0,  0,
         3,  3,  3,  3,  3,  3,  3,  3,  3,  3,  3,  3,  3,  3,  3,  0,  0, 21,  0,  0,  0, 23,  0,  0,  0,  0,  0,  0,  0,  0,  0,  2,  2,  2,  2,  2,  2,  2,  2,  2,  3,  3,  3,  0, 21,  0,  0,  0,  0,  0,  7,  8,  7,  8,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0, 20,  0,  0,  0,  0,  0,  0,  0,  0,  0,  7,  8,  7,  8,  0, 21,  0,  0,  3,  3,
         2,  2,  2,  2,  2,  2,  2,  2,  2,  2,  2,  2,  2,  2,  2,  3,  3,  3,  3,  3,  3,  2,  0,  0,  0,  0,  0,  0,  0,  0,  0,  2,  2,  2,  2,  2,  2,  2,  2,  2,  2,  2,  2,  3,  3,  3,  0, 22,  0,  0,  0,  0,  0, 21,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0, 22,  0,  0,  0,  0,  0, 20,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  3,  3,  3,  2,  2,
         2,  2,  2,  2,  2,  2,  2,  2,  2,  2,  2,  2,  2,  2,  2,  2,  2,  2,  2,  2,  2,  2,  9,  9,  9,  9,  9,  9,  9,  9,  9,  6,  6,  6,  6,  6,  6,  6,  6,  6,  2,  2,  2,  2,  2,  2,  3,  3,  3,  3,  3,  3,  3,  3,  3,  3,  3, 23, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 24,  3,  3,  3,  3,  0,  0, 20,  0,  0, 21,  0,  0,  0, 22,  0,  0,  0,  0,  3,  3,  3,  2,  2,  2,  2,  2,
         6,  6,  6,  6,  6,  6,  6,  6,  6,  6,  6,  6,  6,  6,  6,  2,  2,  2,  2,  2,  2,  6,  9,  9,  9,  9,  9,  9,  9,  9,  9,  5,  5,  5,  5,  5,  5,  5,  5,  5,  6,  6,  6,  2,  2,  2,  2,  2,  2,  2,  2,  2,  2,  2,  2,  2,  2,  2, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13,  2,  2,  2,  2,  2,  3,  3,  3,  3,  3,  3,  3,  3,  3,  3,  3,  3,  3,  3,  2,  2,  2,  2,  2,  2,  6,  6,
         4,  4,  4,  4,  4,  4,  4,  4,  4,  4,  4,  4,  4,  4,  4,  6,  6,  6,  6,  6,  6,  4,  9,  9,  9,  9,  9,  9,  9,  9,  9,  4,  4,  4,  4,  4,  4,  4,  4,  4,  4,  4,  4,  6,  6,  6,  4,  4,  4,  4,  4,  4,  4,  4,  4,  4,  4,  4, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13,  4,  4,  4,  4,  4,  4,  4,  4,  4,  4,  4,  4,  4,  4,  4,  4,  4,  4,  4,  4,  4,  4,  6,  6,  6,  4,  4,
         4,  4,  4,  4,  4,  4,  4,  4,  4,  4,  4,  4,  4,  4,  4,  4,  4,  4,  4,  4,  4,  4,  9,  9,  9,  9,  9,  9,  9,  9,  9,  4,  4,  4,  4,  4,  4,  4,  4,  4,  4,  4,  4,  4,  4,  4,  6,  6,  6,  6,  6,  6,  6,  6,  6,  6,  6,  6, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13,  6,  6,  6,  6,  6,  4,  4,  4,  4,  4,  4,  4,  4,  4,  4,  4,  4,  4,  4,  6,  6,  6,  4,  4,  4,  4,  4,
      },
    },
  },
}

local TERRAIN = LEVEL.layers[1]
local LEVEL_W, LEVEL_H = TERRAIN.width, TERRAIN.height
local LEVEL_PX = LEVEL_W * LEVEL.tilewidth   -- 768 px of level...
local MAP_COLS = 64                          -- ...over a 512 px (64 tile) tilemap
local MAP_TOP = 16                           -- terrain sits on screen tile rows 16..27
local SPEED = 64                             -- camera pixels per second
local ANIM_HZ = 6                            -- animated-tile steps per second

-- Tiled numbers tiles from the tileset's firstgid (1 here) and writes 0 for an
-- empty cell. A sheet numbers chars from 0 with NO reserved blank tile, so the
-- adapter is just gid - 1 -- and an empty Tiled cell lands on sheet cell 0, which
-- this sheet leaves blank on purpose.
local function gid_to_tile(gid)
  if gid == 0 then return 0 end
  return gid - 1
end

-- Animated materials. The level stores ONE gid per material; the frame picks the
-- variant. Variants are consecutive sheet cells, so a cycle is a single add.
-- Pokes re-run every frame, which is the whole animation mechanism.
local ANIM = {
  [9]  = { first = 8,  frames = 4 },   -- lava  -> cells 8..11
  [13] = { first = 12, frames = 4 },   -- water -> cells 12..15
}

function frame(t, f)
  apply_pokes()
  mode = 1; brightness = 15

  -- BG1 = the tilesheet. Binding a sheet places its chars and palettes and echoes
  -- back char_base ONLY: map_base, screen_size and tile_size stay yours, and BG1
  -- rasterizes at map_base 0 by default, so say what you mean.
  bg[1].source = "cavern_tiles"
  bg[1].char_base = 0x1000             -- default, shown explicitly
  bg[1].map_base = 0x0000              -- default, shown explicitly
  bg[1].screen_size = 1                -- 64x32 tiles = 512x256 px

  -- BG2 = an ordinary assembled import, which brings its own tilemap and only
  -- needs somewhere to live. Both source kinds, one frame.
  --
  -- SWAPPING THE ART? Four constraints below bite SILENTLY -- each one gives a
  -- wrong picture, never an error:
  --  1. Both images must use the SAME SET of colours, and that set must fit ONE
  --     sub-palette (15 at 4bpp). Outside mode 0 every BG source lands its
  --     palettes at CGRAM 0 and BG2 is placed after BG1, so a backdrop with a
  --     different palette silently recolours the tilesheet layer.
  --  2. pal = 0 in the map write below assumes that single sub-palette. A sheet
  --     needing several puts each cell's index in the import report (the source
  --     preview labels it) -- there is no way to read it back from Lua.
  --  3. char_base 0x1000 -> 0x2000 leaves the sheet 256 chars at 4bpp. A bigger
  --     sheet overruns BG2's chars; move BG2 up.
  --  4. MAP_TOP + LEVEL_H must be <= 32, the tilemap's height in tiles.
  bg[2].source = "cavern_back"
  bg[2].char_base = 0x2000
  bg[2].map_base = 0x0800

  -- The camera walks the level and loops. The scroll register only ever sees it
  -- mod 512 -- the tilemap's own width -- which is the fine (sub-tile) scroll.
  local cam = (t * SPEED) % LEVEL_PX
  local cam_tile = floor(cam / LEVEL.tilewidth)
  bg[1].scroll.x = cam % 512
  -- Parallax at exactly 1/3: the backdrop is 256 px wide and the level 768, so
  -- one level loop is three backdrop loops and the wrap never shows.
  bg[2].scroll.x = cam / 3

  local phase = floor(t * ANIM_HZ)

  -- Coarse streaming. 33 columns = the 32 on screen plus the partial one the fine
  -- scroll pulls in. Tilemap column (cam_tile + s) % 64 is exactly the column the
  -- rasterizer reads for screen column s, so crossing 512 px needs no special
  -- case; and a column outside this window can never be on screen, which is why
  -- writing only the window is enough. (VRAM is zeroed every frame before
  -- imports, so the window is rewritten each frame rather than patched.)
  for s = 0, 32 do
    local col = (cam_tile + s) % LEVEL_W     -- column of the LEVEL
    local mcol = (cam_tile + s) % MAP_COLS   -- column of the TILEMAP
    if bg[1].map[mcol] == nil then bg[1].map[mcol] = {} end
    for r = 0, LEVEL_H - 1 do
      local gid = TERRAIN.data[r * LEVEL_W + col + 1]   -- Tiled data: row-major, 1-based
      local anim = ANIM[gid]
      local tile
      if anim then
        tile = anim.first + phase % anim.frames
      else
        tile = gid_to_tile(gid)
      end
      -- One line per map entry: the editor completes tile/pal/prio/flip_x/flip_y
      -- inside a single-line constructor.
      bg[1].map[mcol][MAP_TOP + r] = { tile = tile, pal = 0 }
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
  demo(
    "offset-per-tile",
    "offset-per-tile",
    [{ name: "main.lua", source: OFFSET_SRC }],
    [ribbons()],
  ),
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
  demo(
    "tilesheet-cavern",
    "tilesheet-cavern",
    [{ name: "main.lua", source: CAVERN_SRC }],
    [cavernTiles(), cavernBack()],
  ),
];

// ── first-run starter ────────────────────────────────────────────────────────
// The template a brand-new visitor boots into. NOT in DEMOS: demos ship to the
// wall as published toys by the official account; this exists only as the
// studio's lazily-forked first-open context. Asset-free on purpose — it must
// render (and animate) with nothing uploaded, while touching the core DSL
// ideas: frame(t,f), per-scanline hdma, cgram, hsl.
const STARTER_SRC = `-- my first toy — you're driving a real SNES picture chip.
-- frame(t, f) runs every frame: t = seconds, f = frame number.
-- Edits re-run live. Fork any toy on the wall to see more tricks.
function frame(t, f)
  apply_pokes()          -- inspector tweaks land here (see pokes.lua)
  mode = 0
  cgram[0] = hsl((t * 24) % 360, 0.55, 0.32)   -- backdrop colour, drifting
  -- hdma runs your function once per scanline: fade to black at the bottom
  hdma(0, 223, function(y)
    brightness = 15 - floor(y / 24)            -- 15 at the top -> 6 low
  end)
  -- Try: swap 0.55 for 1, speed up the 24, or flip it: floor((223-y)/24)
end
`;

export const STARTER: Demo = demo(
  "starter",
  "my first toy",
  [{ name: "main.lua", source: STARTER_SRC }],
  [],
);

/** Resolve a demo-context id: the bundled DEMOS plus the starter template. */
export function demoById(id: string): Demo | undefined {
  if (id === STARTER.id) return STARTER;
  return DEMOS.find((d) => d.id === id);
}
