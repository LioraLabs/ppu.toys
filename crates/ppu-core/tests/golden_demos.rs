//! Flagship demo golden tests through the real Lua/importer/render pipeline.
use ppu_core::{
    convert_source, render_frame, ConvertOptions, ImportBudget, LuaEngine, SourceKind, HEIGHT,
    WIDTH,
};
use std::path::Path;

const CAVERN_GOLDEN: &str = "tests/fixtures/golden_tilesheet_cavern.png";
const DUSK_GOLDEN: &str = "tests/fixtures/golden_dusk_parallax.png";
const MODE7_GOLDEN: &str = "tests/fixtures/golden_mode7_floor.png";
const OFFSET_GOLDEN: &str = "tests/fixtures/golden_offset_per_tile.png";
const MODE3_GOLDEN: &str = "tests/fixtures/golden_mode3_gradient.png";
const MODE0_GOLDEN: &str = "tests/fixtures/golden_mode0_bands.png";
const TRANSLUCENCY_GOLDEN: &str = "tests/fixtures/golden_translucency.png";
const SPOTLIGHT_GOLDEN: &str = "tests/fixtures/golden_spotlight.png";
const GLOW_GOLDEN: &str = "tests/fixtures/golden_glow.png";
const TM_MASK_GOLDEN: &str = "tests/fixtures/golden_tm_mask.png";
const SHADOW_GOLDEN: &str = "tests/fixtures/golden_shadow.png";
const SPRITE_STORM_GOLDEN: &str = "tests/fixtures/golden_sprite_storm.png";
const MOSAIC_GOLDEN: &str = "tests/fixtures/golden_mosaic.png";
const EXTBG_GOLDEN: &str = "tests/fixtures/golden_extbg.png";
const DIRECT_GOLDEN: &str = "tests/fixtures/golden_direct_color.png";

const DUSK_MAIN_SRC: &str = r#"-- ppu.toys :: dusk-parallax (Mode 1: parallax BG scroll + CGRAM colour-cycle + sprite)
-- Multi-file flagship: SPEED + dusk_palette() live in palette.lua. Chunks run in
-- tab order into ONE shared global scope; frame() resolves after all chunks, so
-- main.lua may reference palette.lua globals freely (main.lua is convention, not magic).
-- Setup stage: dma() runs once, from top-level code, placing each source at an
-- explicit VRAM/CGRAM address. Nothing is echoed back -- frame() points the
-- layer registers at the same addresses, like the chip does.
dma("sky", { char = 0x1000, map = 0x0000 })
dma("hills", { char = 0x4000, map = 0x0800 })
dma("hero", { char = 0x6000 })
function frame(t, f)
  apply_pokes()
  mode = 1; brightness = 15
  bg[1].char_base = 0x1000; bg[1].map_base = 0x0000
  bg[2].char_base = 0x4000; bg[2].map_base = 0x0800
  bg[1].scroll.x = t * SPEED
  bg[2].scroll.x = t * SPEED * 3
  dusk_palette(t)
  obj[0].tile = 4; obj[0].pal = 0; obj[0].prio = 3; obj[0].x = 120; obj[0].y = 132 + sin(t*3) * 4
  obj.char_base = 0x6000; obj[0].on = true
end
"#;

const DUSK_PALETTE_SRC: &str = r#"-- dusk-parallax :: palette.lua — CGRAM colour-cycle ($40-$47), globals shared with main.lua
SPEED = 12
function dusk_palette(t)
  for i = 0, 7 do cgram[0x40 + i] = hsl((t*40 + i*12) % 360, 0.6, 0.5) end
end
"#;

/// The single-file concat of the flagship's USER chunks (main + palette) for the
/// multi-file parity golden. The pokes chunk is not part of this concat — it is
/// prepended separately by `demo_engine_files`, mirroring web tab order.
fn dusk_concat() -> String {
    format!("{DUSK_MAIN_SRC}\n{DUSK_PALETTE_SRC}")
}

const MODE7_SRC: &str = r#"-- ppu.toys :: mode7-floor (the namesake; per-scanline affine floor)
-- An m7 payload always lives interleaved at VRAM 0x0000 on real hardware, so
-- dma takes no address for it.
dma("track")
function frame(t, f)
  apply_pokes()
  mode = 7; brightness = 15
  hdma(96, 223, function(y)
    local d = 64 / (y - 95)
    m7.a, m7.d = d, d
    m7.cx, m7.cy = 128, 0
    bg[1].scroll.y = (t*80) * d
  end)
end
"#;

const OFFSET_SRC: &str = r#"-- ppu.toys :: offset-per-tile (Mode 2: BG3 table drives per-column scroll)
dma("ribbons", { char = 0x1000, map = 0x0000 })

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
  bg[1].char_base = 0x1000; bg[1].map_base = 0x0000
  bg[3].map_base = 0x0800
  for col = 0, 31 do
    local wave = floor((sin((col + t * 8) / 3) + 1) * 4)
    column_offset(col, wave, col % 3)
  end
end
"#;

const MODE3_SRC: &str = r#"-- ppu.toys :: mode3-gradient (Mode 3: 8bpp 256-colour BG1 gradient)
dma("gradient", { char = 0x1000, map = 0x0000 })
function frame(t, f)
  apply_pokes()
  mode = 3; brightness = 15
  bg[1].char_base = 0x1000; bg[1].map_base = 0x0000
end
"#;

const MODE0_SRC: &str = r#"-- ppu.toys :: mode0-bands (Mode 0: two 2bpp layers, per-layer CGRAM band)
-- pal picks each source's CGRAM band explicitly (the mode-0 32-per-layer
-- banding the old bind path applied automatically, spelled out).
dma("mode0_bg1", { char = 0x1000, map = 0x0000, pal = 0 })
dma("mode0_bg2", { char = 0x2000, map = 0x0400, pal = 32 })
function frame(t, f)
  mode = 0; brightness = 15
  bg[1].char_base = 0x1000; bg[1].map_base = 0x0000
  bg[2].char_base = 0x2000; bg[2].map_base = 0x0400
end
"#;

const TRANSLUCENCY_SRC: &str = r#"-- ppu.toys :: translucency (½-add glass panel over a scrolling BG)
dma("panel", { char = 0x1000, map = 0x0000 })
dma("ribbons", { char = 0x2000, map = 0x0800 })
function frame(t, f)
  apply_pokes()
  mode = 1; brightness = 15
  bg[1].char_base = 0x1000; bg[1].map_base = 0x0000   -- the glass panel (main only)
  bg[2].char_base = 0x2000; bg[2].map_base = 0x0800   -- scene, on main AND sub
  screen.main.bg1 = true; screen.main.bg2 = true      -- panel + scene on the main screen
  screen.main.bg3 = false; screen.main.bg4 = false; screen.main.obj = false  -- power-on defaults ALL layers on: drop the rest
  screen.sub.bg2 = true    -- scene on the sub screen -> the addend under the glass
  color.op = "add"; color.half = true; color.on.bg1 = true  -- ½-add math on BG1 (the glass)
  color.addend = "sub"     -- addend = subscreen (not fixed colour)
end
"#;

const SPOTLIGHT_SRC: &str = r#"-- ppu.toys :: spotlight (per-scanline circular iris via the colour window)
dma("ribbons", { char = 0x1000, map = 0x0000 })
function frame(t, f)
  apply_pokes()
  mode = 1; brightness = 15
  bg[1].char_base = 0x1000; bg[1].map_base = 0x0000
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
"#;

const GLOW_SRC: &str = r#"-- ppu.toys :: additive-glow (fixed-colour add brightens BG1 toward warm)
dma("ribbons", { char = 0x1000, map = 0x0000 })
function frame(t, f)
  apply_pokes()
  mode = 1; brightness = 15
  bg[1].char_base = 0x1000; bg[1].map_base = 0x0000
  screen.main.bg1 = true    -- BG1 only on the main screen
  screen.main.bg2 = false; screen.main.bg3 = false   -- power-on defaults ALL layers on: drop the rest
  screen.main.bg4 = false; screen.main.obj = false
  color.op = "add"; color.on.bg1 = true   -- add at full strength (half stays off)
  color.addend = "fixed"    -- addend = the fixed colour, not the sub screen
  color.fixed = rgb(120, 60, 0)  -- warm glow added to every BG1 pixel
end
"#;

const TM_MASK_SRC: &str = r#"-- ppu.toys :: tm-mask (TM drops BG2 from the main screen)
function frame(t, f)
  mode = 0; brightness = 15
  bg[1].source = "mode0_bg1"
  bg[2].source = "mode0_bg2"; bg[2].map_base = 0x0400; bg[2].char_base = 0x2000
  TM = 0x01   -- BG1 only; BG2 is masked off the main screen
end
"#;

const SHADOW_SRC: &str = r#"-- ppu.toys :: shadow (subtractive fixed-colour darkens BG1)
function frame(t, f)
  mode = 1; brightness = 15
  bg[1].source = "ribbons"
  TM = 0x01
  CGADSUB = 0x81          -- subtract (bit7) + BG1 math-enable
  CGWSEL = 0x00           -- addend = COLDATA fixed colour
  COLDATA = rgb(120, 120, 120)
end
"#;

const SPRITE_STORM_SRC: &str = r#"-- ppu.toys :: sprite-storm (authentic OBJ flicker: >32 sprites on one band, OAM start rotates each frame)
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
"#;

const MOSAIC_SRC: &str = r#"-- ppu.toys :: mosaic (BG1 pixelation; block size steps every 8 frames)
dma("ramp", { char = 0x1000, map = 0x0000 })
function frame(t, f)
  apply_pokes()
  mode = 3; brightness = 15
  bg[1].char_base = 0x1000; bg[1].map_base = 0x0000
  bg[1].mosaic = true
  mosaic = floor(f / 8) % 16
end
"#;

const EXTBG_SRC: &str = r#"-- ppu.toys :: mode7-extbg (per-pixel floor priority; sprite between the two levels)
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
"#;

const DIRECT_SRC: &str = r#"-- ppu.toys :: direct-color (8bpp Mode 7, CGRAM bypass, smooth colour field)
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
"#;

const CAVERN_SRC: &str = r#"-- ppu.toys :: tilesheet-cavern (Mode 1: a camera streaming a Tiled-authored map
-- out of a tilesheet, with animated lava/water, over an assembled-import backdrop)
--
-- The reference implementation of the tilesheet workflow:
--   1. dma the sheet at setup        -- chars land in sheet order: tile N = cell N
--   2. set the map geometry YOURSELF -- placement is explicit; nothing is echoed
--   3. rewrite bg[1].map each frame  -- the 64x32 tilemap IS the camera's window
--
-- LEVEL is a Tiled Lua export (File > Export As > Lua) with its 'data' array kept
-- whole, one map row per line as Tiled writes it. To use your own map, replace
-- this table -- and then check the four things below it that are level-specific:
-- ANIM's keys (gids of THIS tileset), MAP_TOP (assumes LEVEL_H rows fit above
-- row 32), pal = 0 in the map write, and the palette constraint on BG2.

-- Setup stage: place both sources once, at explicit addresses. A sheet is
-- chars+palette only (no map); the assembled backdrop brings its own tilemap.
dma("cavern_tiles", { char = 0x1000 })
dma("cavern_back", { char = 0x2000, map = 0x0800 })

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

  -- BG1 = the tilesheet. The dma placed its chars and palette; the map geometry
  -- is all yours -- and BG1 rasterizes at map_base 0 by default, so say what
  -- you mean.
  bg[1].char_base = 0x1000             -- where the dma put the chars
  bg[1].map_base = 0x0000              -- default, shown explicitly
  bg[1].screen_size = 1                -- 64x32 tiles = 512x256 px

  -- BG2 = an ordinary assembled import, which brings its own tilemap and only
  -- needs somewhere to live. Both source kinds, one frame.
  --
  -- SWAPPING THE ART? Four constraints below bite SILENTLY -- each one gives a
  -- wrong picture, never an error:
  --  1. Both images must use the SAME SET of colours, and that set must fit ONE
  --     sub-palette (15 at 4bpp). Both dma calls land their palettes at CGRAM 0
  --     (pal defaults to 0) and the backdrop is placed after the sheet, so a
  --     backdrop with a different palette silently recolours the tilesheet layer.
  --  2. pal = 0 in the map write below assumes that single sub-palette. A sheet
  --     needing several puts each cell's index in the import report (the source
  --     preview labels it) -- there is no way to read it back from Lua.
  --  3. char 0x1000 -> 0x2000 leaves the sheet 256 chars at 4bpp. A bigger
  --     sheet overruns BG2's chars; move BG2 up.
  --  4. MAP_TOP + LEVEL_H must be <= 32, the tilemap's height in tiles.
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
  -- writing only the window is enough. (VRAM is zeroed and the dma placements
  -- replayed every frame, so the window is rewritten each frame rather than
  -- patched.)
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
"#;

fn ramp() -> Vec<u8> {
    let mut data = vec![0u8; WIDTH * HEIGHT * 4];
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let i = (y * WIDTH + x) * 4;
            data[i] = ((x % 32) * 8) as u8;
            data[i + 1] = ((y % 32) * 8) as u8;
            data[i + 2] = 128;
            data[i + 3] = 255;
        }
    }
    data
}

fn sky() -> Vec<u8> {
    const W: usize = WIDTH;
    const H: usize = HEIGHT;
    const HORIZON: usize = 140;
    let mut data = vec![0u8; W * H * 4];
    for y in 0..H {
        for x in 0..W {
            let i = (y * W + x) * 4;
            if y >= HORIZON {
                data[i + 3] = 0;
                continue;
            }
            let dx = x as i32 - 192;
            let dy = y as i32 - 50;
            if dx * dx + dy * dy < 20 * 20 {
                data[i..i + 4].copy_from_slice(&[255, 226, 168, 255]);
                continue;
            }
            let t = y as f32 / HORIZON as f32;
            data[i] = 30 + (t * t * 210.0).round() as u8;
            data[i + 1] = 18 + (t * 70.0).round() as u8;
            data[i + 2] = 78 + (t * 52.0).round() as u8;
            data[i + 3] = 255;
        }
    }
    data
}

fn hills() -> Vec<u8> {
    const W: usize = WIDTH;
    const H: usize = HEIGHT;
    const TOP: usize = 138;
    let mut data = vec![0u8; W * H * 4];
    for y in 0..H {
        for x in 0..W {
            let i = (y * W + x) * 4;
            if y < TOP {
                data[i + 3] = 0;
                continue;
            }
            let stripe = (x / 16) % 2;
            let d = (y - TOP) as f32 / (H - TOP) as f32;
            data[i] = 18 + stripe as u8 * 10;
            data[i + 1] = 96 - (d * 46.0).round() as u8 + stripe as u8 * 12;
            data[i + 2] = 38 + stripe as u8 * 8;
            data[i + 3] = 255;
        }
    }
    data
}

fn hero() -> Vec<u8> {
    let (w, h) = (64usize, 8usize);
    let mut data = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) * 4;
            let cell = x / 8;
            data[i] = 255 - cell as u8 * 16;
            data[i + 1] = 200;
            data[i + 2] = cell as u8 * 24;
            data[i + 3] = 255;
        }
    }
    data
}

fn track() -> Vec<u8> {
    let (w, h) = (1024usize, 1024usize);
    let mut data = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let (cx, cy) = ((x / 8) % 8, (y / 8) % 8);
            let i = (y * w + x) * 4;
            data[i] = cx as u8 * 32;
            data[i + 1] = cy as u8 * 32;
            data[i + 2] = (((cx + cy) & 1) * 255) as u8;
            data[i + 3] = 255;
        }
    }
    data
}

fn ribbons() -> Vec<u8> {
    let mut data = vec![0u8; WIDTH * HEIGHT * 4];
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let i = (y * WIDTH + x) * 4;
            let band = ((x / 8) % 8) as u8;
            data[i] = 32 + band * 24;
            data[i + 1] = 40 + ((y / 8) % 8) as u8 * 24;
            data[i + 2] = 220 - band * 16;
            data[i + 3] = 255;
        }
    }
    data
}

fn panel() -> Vec<u8> {
    let mut data = vec![0u8; WIDTH * HEIGHT * 4];
    for y in 0..HEIGHT {
        let opaque = (80..160).contains(&y);
        for x in 0..WIDTH {
            let i = (y * WIDTH + x) * 4;
            if opaque {
                data[i..i + 4].copy_from_slice(&[80, 230, 255, 255]); // cyan glass
            } // else alpha 0
        }
    }
    data
}

fn gradient() -> Vec<u8> {
    let mut data = vec![0u8; WIDTH * HEIGHT * 4];
    for y in 0..HEIGHT {
        // top->bottom hue sweep; constant across x so unique tiles stay bounded.
        let r = (y * 255 / (HEIGHT - 1)) as u8;
        let g = ((HEIGHT - 1 - y) * 255 / (HEIGHT - 1)) as u8;
        for x in 0..WIDTH {
            let i = (y * WIDTH + x) * 4;
            data[i] = r;
            data[i + 1] = g;
            data[i + 2] = 128;
            data[i + 3] = 255;
        }
    }
    data
}

fn mode0_bg1() -> Vec<u8> {
    let mut data = vec![0u8; WIDTH * HEIGHT * 4];
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let i = (y * WIDTH + x) * 4;
            if (x / 8) % 2 == 0 {
                data[i..i + 4].copy_from_slice(&[40, 220, 90, 255]); // green
            } // else alpha 0 = transparent
        }
    }
    data
}

fn mode0_bg2() -> Vec<u8> {
    let mut data = vec![0u8; WIDTH * HEIGHT * 4];
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let i = (y * WIDTH + x) * 4;
            if (y / 8) % 2 == 0 {
                data[i..i + 4].copy_from_slice(&[220, 60, 200, 255]); // magenta
            }
        }
    }
    data
}

// ── cavern (PPU-95): the tilesheet demo's two sources ───────────────────────
// Mirrors web/src/studio/demos/demos.ts cavernTiles()/cavernBack() exactly —
// these two MUST agree or the golden proves nothing about the shipped demo.
// One 14-colour master palette feeds both images on purpose: both dma calls
// land their palettes at CGRAM 0 and the backdrop is placed after the sheet, so
// two different palettes would clobber each other. The invariant has TWO halves --
// an identical colour SET, and that set fitting ONE sub-palette (15 at 4bpp).
// Sorting alone is not enough: past one sub-palette region_fit partitions
// greedily in tile order, so one colour set can split differently between two
// images. 14 <= 15 is load-bearing, not slack.
// `cavern_backdrop_import_does_not_recolour_the_tilesheet_layer` pins the CGRAM.
const CAVERN_PAL: [(u8, u8, u8); 15] = [
    (0x00, 0x00, 0x00), // 0 = transparent, never placed
    (0x10, 0x18, 0x28), //  1
    (0x28, 0x38, 0x60), //  2
    (0x48, 0x70, 0xa8), //  3
    (0x78, 0xc0, 0xe0), //  4
    (0x30, 0x28, 0x20), //  5
    (0x60, 0x48, 0x30), //  6
    (0x98, 0x70, 0x48), //  7
    (0x38, 0x38, 0x40), //  8
    (0x58, 0x58, 0x68), //  9
    (0x90, 0x90, 0x98), // 10
    (0xb8, 0x38, 0x10), // 11
    (0xf0, 0x70, 0x18), // 12
    (0xff, 0xc8, 0x50), // 13
    (0x58, 0xa8, 0x48), // 14
];

/// 24 8x8 cells in row-major SHEET order: cell N is what `tile = N` draws.
/// 64 hex palette indices per cell, whitespace stripped, '0' = transparent.
const CAVERN_CELLS: [&str; 24] = [
    //  0 blank (the author-reserved empty tile: a sheet has no built-in one)
    r"
      00000000
      00000000
      00000000
      00000000
      00000000
      00000000
      00000000
      00000000
    ",
    //  1 rock fill
    r"
      99a99998
      9a999899
      99989a99
      89999a98
      999a8999
      9998999a
      9a999998
      899a9999
    ",
    //  2 rock top (moss cap)
    r"
      eeeeeeee
      9ee9eee9
      a99a99a9
      99a99999
      9899999a
      99998999
      9a999998
      99989a99
    ",
    //  3 rock dark (bedrock)
    r"
      88818888
      81888818
      88888188
      88188888
      88888818
      18888188
      88818888
      88888881
    ",
    //  4 dirt fill
    r"
      66576665
      66666756
      57666665
      66566766
      66675666
      56666657
      66566666
      66666576
    ",
    //  5 dirt top
    r"
      77777777
      67767677
      66676666
      66666576
      56666666
      66657666
      66666665
      65666676
    ",
    //  6 brick course A
    r"
      aaaaaaaa
      a999999a
      a999999a
      88888888
      aaaaaaaa
      99a99999
      99999a99
      88888888
    ",
    //  7 brick course B
    r"
      9a9988aa
      999988a9
      88888888
      aa999999
      a9999999
      88888888
      9988aa99
      9988a999
    ",
    //  8 lava frame 0
    r"
      bbbbbbbb
      bcbbbbcb
      bccbbccb
      cccdcccc
      ccdddccc
      cdddddcc
      dddddddd
      dddddddd
    ",
    //  9 lava frame 1
    r"
      bbbbbbbb
      bbcbbcbb
      bcccbccb
      ccccdccc
      cccdddcc
      ccdddddc
      dddddddd
      dddddddd
    ",
    // 10 lava frame 2
    r"
      bbbbbbbb
      bbbcbcbb
      bbccbcbb
      cccccdcc
      ccccdddc
      cccddddd
      dddddddd
      dddddddd
    ",
    // 11 lava frame 3
    r"
      bbbbbbbb
      cbbbcbbc
      ccbbccbc
      ccccccdc
      cccccddd
      ccccddcd
      dddddddd
      dddddddd
    ",
    // 12 water frame 0
    r"
      44433334
      33333333
      33232333
      32222233
      22222222
      22122222
      21112221
      11111111
    ",
    // 13 water frame 1
    r"
      34443333
      33333333
      33323233
      33222223
      22222222
      22212222
      12111222
      11111111
    ",
    // 14 water frame 2
    r"
      33444333
      33333333
      23332323
      33322222
      22222222
      22221222
      22121112
      11111111
    ",
    // 15 water frame 3
    r"
      33344433
      33333333
      32333232
      23332222
      22222222
      22222122
      22212111
      11111111
    ",
    // 16 ledge left
    r"
      000aaaaa
      00aa999a
      0aa99999
      aa999998
      a9999899
      a9989999
      89999998
      89999889
    ",
    // 17 ledge mid
    r"
      aaaaaaaa
      a99999a9
      999a9999
      99899999
      99998999
      89999998
      99899899
      98999899
    ",
    // 18 ledge right
    r"
      aaaaa000
      a999aa00
      99999aa0
      899999aa
      9989999a
      9999899a
      89999998
      98899998
    ",
    // 19 pillar
    r"
      0aaaaaa0
      0a9999a0
      0a9889a0
      0a9889a0
      0a9889a0
      0a9889a0
      0a9999a0
      0aaaaaa0
    ",
    // 20 crystal
    r"
      00044000
      00434400
      04333440
      04333340
      03333340
      03323330
      00332300
      00033000
    ",
    // 21 pebbles
    r"
      00000000
      00090000
      000a9000
      00000000
      0900009a
      0a90000a
      00000000
      0000a900
    ",
    // 22 rock top-left corner
    r"
      0000eeee
      000ee99e
      00ee999a
      0ee99999
      ee999a99
      e99999a9
      9998999a
      999a9999
    ",
    // 23 rock top-right corner
    r"
      eeee0000
      e99ee000
      a999ee00
      99999ee0
      99a999ee
      9a99999e
      a9998999
      9999a999
    ",
];

fn cavern_px(buf: &mut [u8], w: usize, x: usize, y: usize, idx: u32) {
    if idx == 0 {
        return; // transparent: leave alpha 0 so the backdrop shows through
    }
    let (r, g, b) = CAVERN_PAL[idx as usize];
    let i = (y * w + x) * 4;
    buf[i..i + 4].copy_from_slice(&[r, g, b, 255]);
}

fn cavern_tiles() -> Vec<u8> {
    let cols = 8usize;
    let (w, h) = (cols * 8, (CAVERN_CELLS.len() / cols) * 8);
    let mut buf = vec![0u8; w * h * 4];
    for (n, cell) in CAVERN_CELLS.iter().enumerate() {
        let s: Vec<u32> = cell
            .chars()
            .filter(|c| !c.is_whitespace())
            .map(|c| c.to_digit(16).unwrap())
            .collect();
        assert_eq!(s.len(), 64, "cavern cell {n} is not 8x8");
        let (ox, oy) = ((n % cols) * 8, (n / cols) * 8);
        for y in 0..8 {
            for x in 0..8 {
                cavern_px(&mut buf, w, ox + x, oy + y, s[y * 8 + x]);
            }
        }
    }
    buf
}

/// Backdrop palette index at (x, y). 32px horizontal period so the 256px plane
/// wraps seamlessly under a plain `scroll`; uses all 14 colours because the
/// shared-palette invariant needs the colour SETS equal, not merely overlapping.
fn cavern_back_index(x: usize, y: usize) -> u32 {
    let px = x % 32;
    let pillar = (10..22).contains(&px);
    let p = |a: u32, b: u32| if pillar { a } else { b };
    if (14..17).contains(&px) && (70..73).contains(&y) {
        return 4; // crystal glints on the pillar face
    }
    if pillar && (126..129).contains(&y) {
        return 14; // moss cap where the distant rock begins
    }
    if pillar && (px == 10 || px == 21) && (96..168).contains(&y) {
        return 10; // lit pillar edge
    }
    if (15..18).contains(&px) && y >= 196 {
        return if y >= 214 {
            13
        } else if y >= 206 {
            12
        } else {
            11
        }; // lava vent
    }
    if pillar && (188..196).contains(&y) {
        return 7; // dirt seam catching the vent light
    }
    if !pillar && (180..188).contains(&y) {
        return 6;
    }
    // Below the moss line everything recedes into the dark, so a gap in the
    // terrain reads as cave depth rather than as a bright slab.
    if y < 56 {
        return p(2, 1);
    }
    if y < 96 {
        return p(3, 2);
    }
    if y < 129 {
        return p(9, 3);
    }
    if y < 168 {
        return p(9, 8);
    }
    if y < 200 {
        return p(8, 5);
    }
    p(5, 1)
}

fn cavern_back() -> Vec<u8> {
    let (w, h) = (WIDTH, HEIGHT);
    let mut buf = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            cavern_px(&mut buf, w, x, y, cavern_back_index(x, y));
        }
    }
    buf
}

/// Empty pokes.lua chunk — mirrors `pokesToLua([])` / `EMPTY_POKES` from
/// web/src/studio/pokes/pokes.ts byte-for-byte. Demos that call apply_pokes()
/// as frame()'s first line need this no-op definition in scope; demos that
/// don't (the Rust-only MODE0/TM_MASK/SHADOW fixtures) simply never call it.
const EMPTY_POKES_SRC: &str = r#"-- pokes.lua · generated by the inspector — read-only.
-- Poke register/CGRAM values in the inspector to fill this in. To save a
-- configuration, copy apply_pokes() into your own file under a new name.
-- Hand-edits here are overwritten by the next poke.
function apply_pokes()
end
"#;

/// Convert an RGBA generator through the format-committed source path and
/// register it under `name` (mirrors the web `convertSource` + `addSource`
/// flow). The committed format MUST match the mode's bind depth for the demo
/// that uses it, or the strict bind validation renders the layer blank.
fn add_bg(e: &mut LuaEngine, name: &str, rgba: Vec<u8>, w: u32, h: u32, bit_depth: u8) {
    let opts = ConvertOptions {
        bit_depth: Some(bit_depth),
        ..Default::default()
    };
    let (payload, _) = convert_source(SourceKind::Bg, &opts, &rgba, w, h).unwrap();
    e.add_source(name, &payload.encode()).unwrap();
}

fn add_m7(e: &mut LuaEngine, name: &str, rgba: Vec<u8>, w: u32, h: u32) {
    let (payload, _) =
        convert_source(SourceKind::M7, &ConvertOptions::default(), &rgba, w, h).unwrap();
    e.add_source(name, &payload.encode()).unwrap();
}

fn add_obj(e: &mut LuaEngine, name: &str, rgba: Vec<u8>, w: u32, h: u32) {
    let opts = ConvertOptions {
        cell_size: Some(8),
        ..Default::default()
    };
    let (payload, _) = convert_source(SourceKind::Obj, &opts, &rgba, w, h).unwrap();
    e.add_source(name, &payload.encode()).unwrap();
}

fn add_sheet(e: &mut LuaEngine, name: &str, rgba: Vec<u8>, w: u32, h: u32, bit_depth: u8) {
    let opts = ConvertOptions {
        bit_depth: Some(bit_depth),
        ..Default::default()
    };
    let (payload, _) = convert_source(SourceKind::Sheet, &opts, &rgba, w, h).unwrap();
    e.add_source(name, &payload.encode()).unwrap();
}

fn demo_engine_files(files: &[(&str, &str)]) -> LuaEngine {
    let mut e = LuaEngine::new();
    add_bg(&mut e, "sky", sky(), WIDTH as u32, HEIGHT as u32, 4);
    add_bg(&mut e, "hills", hills(), WIDTH as u32, HEIGHT as u32, 4);
    add_obj(&mut e, "hero", hero(), 64, 8);
    add_m7(&mut e, "track", track(), 1024, 1024);
    add_bg(&mut e, "ribbons", ribbons(), WIDTH as u32, HEIGHT as u32, 4);
    add_bg(
        &mut e,
        "gradient",
        gradient(),
        WIDTH as u32,
        HEIGHT as u32,
        8,
    );
    add_bg(
        &mut e,
        "mode0_bg1",
        mode0_bg1(),
        WIDTH as u32,
        HEIGHT as u32,
        2,
    );
    add_bg(
        &mut e,
        "mode0_bg2",
        mode0_bg2(),
        WIDTH as u32,
        HEIGHT as u32,
        2,
    );
    add_bg(&mut e, "panel", panel(), WIDTH as u32, HEIGHT as u32, 4);
    add_bg(&mut e, "ramp", ramp(), WIDTH as u32, HEIGHT as u32, 8);
    add_sheet(&mut e, "cavern_tiles", cavern_tiles(), 64, 24, 4);
    add_bg(
        &mut e,
        "cavern_back",
        cavern_back(),
        WIDTH as u32,
        HEIGHT as u32,
        4,
    );
    let mut chunks = Vec::with_capacity(files.len() + 1);
    chunks.push(("pokes.lua", EMPTY_POKES_SRC));
    chunks.extend_from_slice(files);
    e.set_sources(&chunks).unwrap();
    e
}

fn demo_engine(src: &str) -> LuaEngine {
    demo_engine_files(&[("source", src)])
}

/// One RGBA pixel at (x, y) in a WIDTH*HEIGHT framebuffer.
fn px(fb: &[u8], x: usize, y: usize) -> &[u8] {
    &fb[(y * WIDTH + x) * 4..][..4]
}

fn render_storm(f: u32) -> (Vec<u8>, ppu_core::ObjOverflow) {
    let mut e = demo_engine(SPRITE_STORM_SRC);
    let lt = e.frame(1.0, f).unwrap();
    ppu_core::render_frame_stats(&lt, e.memory())
}

fn render_demo(src: &str) -> (Vec<u8>, LuaEngine) {
    let mut e = demo_engine(src);
    let lt = e.frame(1.0, 60).unwrap();
    let fb = render_frame(&lt, e.memory());
    (fb, e)
}

fn decode_png(path: &str) -> Vec<u8> {
    let decoder = png::Decoder::new(std::fs::File::open(path).unwrap());
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    buf.truncate(info.buffer_size());
    buf
}

fn write_png(path: &str, fb: &[u8]) {
    std::fs::create_dir_all("tests/fixtures").unwrap();
    let file = std::fs::File::create(path).unwrap();
    let mut encoder = png::Encoder::new(file, WIDTH as u32, HEIGHT as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(fb)
        .unwrap();
}

#[test]
fn dusk_parallax_uses_bg_imports_and_obj_import() {
    let (fb, e) = render_demo(&dusk_concat());
    // The dma placements (sky/hills 4bpp BG, hero OBJ) replay cleanly: no
    // Mismatch report (a source removed after placement would push one).
    assert!(
        !e.import_reports()
            .iter()
            .any(|r| matches!(r, ImportBudget::Mismatch { .. })),
        "a bound source mismatched its target slot"
    );
    assert!(e.memory().oam[0].on);
    assert!(fb
        .chunks_exact(4)
        .any(|px| px[3] == 255 && px[..3] != [0, 0, 0]));
}

#[test]
fn dusk_parallax_draws_sky_above_horizon() {
    let (fb, _) = render_demo(&dusk_concat());
    let px = &fb[(20 * WIDTH + 20) * 4..][..4];
    assert_ne!(px, &[0, 0, 0, 255], "sky pixel was backdrop black");
}

#[test]
fn dusk_parallax_draws_obj_sprite_over_hills() {
    let (fb, _) = render_demo(&dusk_concat());
    let lower_half_has_sprite_yellow = (120..155).any(|y| {
        (0..WIDTH).any(|x| {
            let p = &fb[(y * WIDTH + x) * 4..][..4];
            p[0] > 180 && p[1] > 150 && p[2] < 80 && p[3] == 255
        })
    });
    assert!(
        lower_half_has_sprite_yellow,
        "OBJ sprite was hidden by BG layers"
    );
}

#[test]
fn mode7_floor_uses_interleaved_mode7_import() {
    let (_fb, e) = render_demo(MODE7_SRC);
    // The Mode 7 `track` dma places without mismatch and lays interleaved char
    // data into the high VRAM byte lane.
    assert!(
        !e.import_reports()
            .iter()
            .any(|r| matches!(r, ImportBudget::Mismatch { .. })),
        "the m7 track source mismatched the Mode 7 slot"
    );
    assert!(e.memory().vram[..64].iter().any(|w| (w >> 8) != 0));
}

#[test]
fn mode7_map_view_is_populated_after_the_floor_demo() {
    let (_fb, e) = render_demo(MODE7_SRC);
    let map = ppu_core::render_mode7_map(e.memory());
    let opaque = map.chunks_exact(4).filter(|px| px[3] == 255).count();
    assert!(
        opaque > 100_000,
        "m7 map view nearly empty: {opaque} opaque px"
    );
}

#[test]
fn mode7_floor_draws_below_horizon() {
    let (fb, _) = render_demo(MODE7_SRC);
    let px = &fb[(160 * WIDTH + 128) * 4..][..4];
    assert_ne!(px, &[0, 0, 0, 255], "floor pixel was backdrop black");
}

#[test]
fn dusk_parallax_demo_matches_golden_png() {
    assert!(Path::new(DUSK_GOLDEN).exists());
    let (actual, _) = render_demo(&dusk_concat());
    let expected = decode_png(DUSK_GOLDEN);
    assert_eq!(actual.len(), expected.len());
    assert!(
        actual == expected,
        "dusk demo framebuffer differs from golden PNG"
    );
}

#[test]
fn mode7_floor_demo_matches_golden_png() {
    assert!(Path::new(MODE7_GOLDEN).exists());
    let (actual, _) = render_demo(MODE7_SRC);
    let expected = decode_png(MODE7_GOLDEN);
    assert_eq!(actual.len(), expected.len());
    assert!(
        actual == expected,
        "mode7 demo framebuffer differs from golden PNG"
    );
}

#[test]
#[ignore = "regenerates the committed dusk demo golden PNG"]
fn regen_golden_dusk_parallax() {
    let (fb, _) = render_demo(&dusk_concat());
    write_png(DUSK_GOLDEN, &fb);
}

#[test]
#[ignore = "regenerates the committed Mode 7 demo golden PNG"]
fn regen_golden_mode7_floor() {
    let (fb, _) = render_demo(MODE7_SRC);
    write_png(MODE7_GOLDEN, &fb);
}

#[test]
fn offset_per_tile_demo_writes_bg3_table_and_draws() {
    let (fb, e) = render_demo(OFFSET_SRC);
    assert_eq!(e.memory().vram[0x0800] & 0x2000, 0x2000);
    assert_eq!(e.memory().vram[0x0800 + 32] & 0xa000, 0xa000);
    assert!(fb
        .chunks_exact(4)
        .any(|px| px[3] == 255 && px[..3] != [0, 0, 0]));
}

#[test]
fn offset_per_tile_demo_matches_golden_png() {
    assert!(Path::new(OFFSET_GOLDEN).exists());
    let (actual, _) = render_demo(OFFSET_SRC);
    let expected = decode_png(OFFSET_GOLDEN);
    assert_eq!(actual.len(), expected.len());
    assert_eq!(
        actual, expected,
        "offset-per-tile demo framebuffer differs from golden PNG"
    );
}

#[test]
#[ignore = "regenerates the committed offset-per-tile demo golden PNG"]
fn regen_golden_offset_per_tile() {
    let (fb, _) = render_demo(OFFSET_SRC);
    write_png(OFFSET_GOLDEN, &fb);
}

#[test]
fn mode3_gradient_demo_imports_bg1_8bpp_and_draws() {
    let (fb, e) = render_demo(MODE3_SRC);
    // The gradient exceeds the 4bpp colour count, so it is committed at 8bpp;
    // binding it into Mode 3's 8bpp BG1 slot must not mismatch (a 4bpp commit
    // would), and the layer must draw.
    assert!(
        !e.import_reports()
            .iter()
            .any(|r| matches!(r, ImportBudget::Mismatch { .. })),
        "the gradient source mismatched the 8bpp BG1 slot"
    );
    assert!(fb
        .chunks_exact(4)
        .any(|px| px[3] == 255 && px[..3] != [0, 0, 0]));
}

#[test]
fn mode3_gradient_demo_matches_golden_png() {
    assert!(Path::new(MODE3_GOLDEN).exists());
    let (actual, _) = render_demo(MODE3_SRC);
    let expected = decode_png(MODE3_GOLDEN);
    assert_eq!(actual.len(), expected.len());
    assert_eq!(
        actual, expected,
        "mode3 demo framebuffer differs from golden PNG"
    );
}

#[test]
#[ignore = "regenerates the committed Mode 3 demo golden PNG"]
fn regen_golden_mode3_gradient() {
    let (fb, _) = render_demo(MODE3_SRC);
    write_png(MODE3_GOLDEN, &fb);
}

#[test]
fn mode0_bands_demo_writes_per_layer_cgram_bands_and_draws() {
    let (fb, e) = render_demo(MODE0_SRC);
    // Both 2bpp layer sources bind into their Mode 0 slots without mismatch.
    assert!(
        !e.import_reports()
            .iter()
            .any(|r| matches!(r, ImportBudget::Mismatch { .. })),
        "a Mode 0 layer source mismatched its 2bpp slot"
    );
    let cg = &e.memory().cgram;
    assert_ne!(cg[1], 0, "BG1 colour missing from band 0");
    assert_ne!(cg[33], 0, "BG2 colour missing from band 1 (offset 32)");
    assert_ne!(cg[1], cg[33], "layers must occupy distinct CGRAM bands");
    assert!(fb
        .chunks_exact(4)
        .any(|px| px[3] == 255 && px[..3] != [0, 0, 0]));
}

#[test]
fn mode0_bands_demo_matches_golden_png() {
    assert!(Path::new(MODE0_GOLDEN).exists());
    let (actual, _) = render_demo(MODE0_SRC);
    let expected = decode_png(MODE0_GOLDEN);
    assert_eq!(actual.len(), expected.len());
    assert_eq!(
        actual, expected,
        "mode0 demo framebuffer differs from golden PNG"
    );
}

#[test]
#[ignore = "regenerates the committed Mode 0 demo golden PNG"]
fn regen_golden_mode0_bands() {
    let (fb, _) = render_demo(MODE0_SRC);
    write_png(MODE0_GOLDEN, &fb);
}

#[test]
fn translucency_demo_blends_panel_half_over_scene() {
    let (fb, _) = render_demo(TRANSLUCENCY_SRC);
    // A column inside the panel band (y=120) blends panel+scene at half; a column
    // in the same x but below the panel (y=200) shows the scene alone.
    let panel_px = &fb[(120 * WIDTH + 128) * 4..][..4];
    let scene_px = &fb[(200 * WIDTH + 128) * 4..][..4];
    assert_ne!(panel_px[..3], [0, 0, 0], "glass pixel went black");
    assert_ne!(scene_px[..3], [0, 0, 0], "scene pixel went black");
    // Half-blend pulls the bright cyan panel toward the darker scene: the blended
    // green channel is below the panel's own ~230 full value.
    assert!(
        panel_px[1] < 230,
        "no half-blend darkening applied to the panel"
    );
}

#[test]
fn translucency_demo_matches_golden_png() {
    assert!(Path::new(TRANSLUCENCY_GOLDEN).exists());
    let (actual, _) = render_demo(TRANSLUCENCY_SRC);
    let expected = decode_png(TRANSLUCENCY_GOLDEN);
    assert_eq!(actual.len(), expected.len());
    assert_eq!(
        actual, expected,
        "translucency demo differs from golden PNG"
    );
}

#[test]
#[ignore = "regenerates the committed translucency demo golden PNG"]
fn regen_golden_translucency() {
    let (fb, _) = render_demo(TRANSLUCENCY_SRC);
    write_png(TRANSLUCENCY_GOLDEN, &fb);
}

#[test]
fn spotlight_demo_masks_scene_to_a_circular_iris() {
    let (fb, _) = render_demo(SPOTLIGHT_SRC);
    // Center of the iris shows the scene; a far corner (well outside r=70) is clipped black.
    let center = &fb[(112 * WIDTH + 128) * 4..][..4];
    let corner = &fb[(5 * WIDTH + 5) * 4..][..4];
    assert_ne!(center[..3], [0, 0, 0], "iris centre was clipped");
    assert_eq!(corner[..3], [0, 0, 0], "outside the iris should be black");
}

/// The Windows editor's per-scanline feed, end to end: the demo's `hdma()` hook
/// really does reach `window_scanline_bytes`, so the panel can draw the iris
/// instead of the two straight lines scanline 0 implies.
#[test]
fn spotlight_window_scanlines_trace_the_iris_chords() {
    let mut e = demo_engine(SPOTLIGHT_SRC);
    let lt = e.frame(1.0, 60).unwrap();
    let bytes = ppu_core::window_scanline_bytes(&lt);
    let stride = ppu_core::WIN_SCANLINE_STRIDE;
    assert_eq!(bytes.len(), HEIGHT * stride);

    // Chord width at row y; <= 0 is the demo's empty span (lo > hi).
    let span = |y: usize| bytes[y * stride + 1] as i32 - bytes[y * stride] as i32 + 1;
    // The demo's circle: cx = 128, cy = 112, r = 70.
    assert_eq!(span(112), 141, "widest chord at the centre row (58..198)");
    assert!(
        span(112) > span(60),
        "the chord narrows away from the centre"
    );
    assert!(span(0) <= 0, "rows above the circle carry an empty span");

    // What the panel's HDMA badge keys off: the edges sweep, the select bytes
    // (index 4 = W12SEL) stay frame-wide.
    let varies = |i: usize| (0..HEIGHT).any(|y| bytes[y * stride + i] != bytes[i]);
    assert!(varies(0) && varies(1), "WH0/WH1 are hdma-driven here");
    assert!(!varies(4), "W12SEL is frame-wide in this demo");
}

#[test]
fn spotlight_demo_matches_golden_png() {
    assert!(Path::new(SPOTLIGHT_GOLDEN).exists());
    let (actual, _) = render_demo(SPOTLIGHT_SRC);
    let expected = decode_png(SPOTLIGHT_GOLDEN);
    assert_eq!(actual.len(), expected.len());
    assert_eq!(actual, expected, "spotlight demo differs from golden PNG");
}

#[test]
#[ignore = "regenerates the committed spotlight demo golden PNG"]
fn regen_golden_spotlight() {
    let (fb, _) = render_demo(SPOTLIGHT_SRC);
    write_png(SPOTLIGHT_GOLDEN, &fb);
}

#[test]
fn glow_demo_adds_fixed_color_over_baseline() {
    let (glow, _) = render_demo(GLOW_SRC);
    // Baseline: identical scene with no colour math.
    let baseline_src = GLOW_SRC
        .replace("color.on.bg1 = true", "color.on.bg1 = false")
        .replace("color.fixed = rgb(120, 60, 0)", "color.fixed = 0");
    let (base, _) = render_demo(&baseline_src);
    // The additive red channel must lift the frame overall (sum of R over the frame).
    let sum_r = |fb: &[u8]| fb.chunks_exact(4).map(|p| p[0] as u64).sum::<u64>();
    assert!(
        sum_r(&glow) > sum_r(&base),
        "additive glow did not brighten the frame"
    );
}

#[test]
fn glow_demo_matches_golden_png() {
    assert!(Path::new(GLOW_GOLDEN).exists());
    let (actual, _) = render_demo(GLOW_SRC);
    let expected = decode_png(GLOW_GOLDEN);
    assert_eq!(actual.len(), expected.len());
    assert_eq!(actual, expected, "glow demo differs from golden PNG");
}

#[test]
#[ignore = "regenerates the committed additive-glow demo golden PNG"]
fn regen_golden_glow() {
    let (fb, _) = render_demo(GLOW_SRC);
    write_png(GLOW_GOLDEN, &fb);
}

#[test]
fn tm_mask_demo_removes_bg2_from_main_screen() {
    let (fb, _) = render_demo(TM_MASK_SRC);
    // mode0_bg2 is magenta (R high, B high, G low). With BG2 masked off TM, no
    // pixel in the frame should read as that magenta.
    let has_magenta = fb
        .chunks_exact(4)
        .any(|p| p[0] > 150 && p[2] > 120 && p[1] < 100 && p[3] == 255);
    assert!(!has_magenta, "BG2 magenta leaked despite TM=0x01");
    // BG1 green is still present.
    let has_green = fb
        .chunks_exact(4)
        .any(|p| p[1] > 150 && p[0] < 100 && p[3] == 255);
    assert!(has_green, "BG1 green missing");
}

#[test]
fn shadow_demo_subtracts_fixed_color_below_baseline() {
    let (shadow, _) = render_demo(SHADOW_SRC);
    let baseline_src = SHADOW_SRC
        .replace("CGADSUB = 0x81", "CGADSUB = 0x00")
        .replace("COLDATA = rgb(120, 120, 120)", "COLDATA = 0");
    let (base, _) = render_demo(&baseline_src);
    let sum = |fb: &[u8]| {
        fb.chunks_exact(4)
            .map(|p| p[0] as u64 + p[1] as u64 + p[2] as u64)
            .sum::<u64>()
    };
    assert!(
        sum(&shadow) < sum(&base),
        "subtract did not darken the frame"
    );
}

#[test]
fn tm_mask_demo_matches_golden_png() {
    assert!(Path::new(TM_MASK_GOLDEN).exists());
    let (actual, _) = render_demo(TM_MASK_SRC);
    let expected = decode_png(TM_MASK_GOLDEN);
    assert_eq!(actual, expected, "tm-mask demo differs from golden PNG");
}

#[test]
#[ignore = "regenerates the committed TM-mask golden PNG"]
fn regen_golden_tm_mask() {
    let (fb, _) = render_demo(TM_MASK_SRC);
    write_png(TM_MASK_GOLDEN, &fb);
}

#[test]
fn shadow_demo_matches_golden_png() {
    assert!(Path::new(SHADOW_GOLDEN).exists());
    let (actual, _) = render_demo(SHADOW_SRC);
    let expected = decode_png(SHADOW_GOLDEN);
    assert_eq!(actual, expected, "shadow demo differs from golden PNG");
}

#[test]
#[ignore = "regenerates the committed subtractive-shadow golden PNG"]
fn regen_golden_shadow() {
    let (fb, _) = render_demo(SHADOW_SRC);
    write_png(SHADOW_GOLDEN, &fb);
}

#[test]
fn sprite_storm_overflows_both_caps_and_flickers() {
    // Both per-line caps engage on the packed band.
    let (fb, ov) = render_storm(90);
    assert!(
        ov.range_over,
        "sprite-storm must exceed the 32-sprite range cap"
    );
    assert!(
        ov.time_over,
        "sprite-storm must exceed the 34-tile time cap"
    );
    assert!(ov.max_sprites > 32);
    // Sprites actually draw over the backdrop.
    assert!(fb
        .chunks_exact(4)
        .any(|p| p[3] == 255 && p[..3] != [0, 0, 0]));
    // Authentic flicker: rotating the OAM start each frame changes the output.
    assert!(
        render_storm(90).0 != render_storm(91).0,
        "OAM rotation must change survivors"
    );
}

#[test]
fn sprite_storm_demo_matches_golden_png() {
    assert!(Path::new(SPRITE_STORM_GOLDEN).exists());
    let mut e = demo_engine(SPRITE_STORM_SRC);
    let lt = e.frame(1.0, 90).unwrap();
    let actual = render_frame(&lt, e.memory());
    let expected = decode_png(SPRITE_STORM_GOLDEN);
    assert_eq!(actual.len(), expected.len());
    assert_eq!(
        actual, expected,
        "sprite-storm demo differs from golden PNG"
    );
}

#[test]
#[ignore = "regenerates the committed sprite-storm demo golden PNG"]
fn regen_golden_sprite_storm() {
    let mut e = demo_engine(SPRITE_STORM_SRC);
    let lt = e.frame(1.0, 90).unwrap();
    write_png(SPRITE_STORM_GOLDEN, &render_frame(&lt, e.memory()));
}

// ── M8 effects demos: mosaic / Mode 7 EXTBG / direct colour ──────────────────

#[test]
fn mosaic_demo_pixelates_bg1_into_8px_blocks() {
    let (fb, _) = render_demo(MOSAIC_SRC);
    // f=60 -> mosaic size 7 -> 8px blocks; each block replicates its top-left texel.
    for &(x, y) in &[(1usize, 0usize), (7, 0), (0, 7), (7, 7)] {
        assert_eq!(
            px(&fb, x, y),
            px(&fb, 0, 0),
            "block(0,0) not flat at ({x},{y})"
        );
    }
    // adjacent block differs (ramp steps within 8px); period-32 block matches.
    assert_ne!(px(&fb, 8, 0), px(&fb, 0, 0), "block(8,0) should differ");
    assert_eq!(
        px(&fb, 32, 0),
        px(&fb, 0, 0),
        "ramp period 32 aligns with blocks"
    );
    // vs mosaic OFF: the fine sub-block detail survives -> the frame differs.
    let off = MOSAIC_SRC.replace("bg[1].mosaic = true", "bg[1].mosaic = false");
    let (base, _) = render_demo(&off);
    assert_ne!(base, fb, "mosaic did not change the frame");
}

#[test]
fn mosaic_demo_matches_golden_png() {
    assert!(Path::new(MOSAIC_GOLDEN).exists());
    let (actual, _) = render_demo(MOSAIC_SRC);
    let expected = decode_png(MOSAIC_GOLDEN);
    assert_eq!(actual.len(), expected.len());
    assert_eq!(actual, expected, "mosaic demo differs from golden PNG");
}

#[test]
#[ignore = "regenerates the committed mosaic demo golden PNG"]
fn regen_golden_mosaic() {
    let (fb, _) = render_demo(MOSAIC_SRC);
    write_png(MOSAIC_GOLDEN, &fb);
}

fn is_red(p: &[u8]) -> bool {
    p[0] > 150 && p[1] < 120 && p[2] < 120 && p[3] == 255
}
fn is_yellow(p: &[u8]) -> bool {
    p[0] > 180 && p[1] > 180 && p[2] < 100 && p[3] == 255
}

#[test]
fn extbg_demo_places_sprite_between_floor_priority_levels() {
    let (fb, _) = render_demo(EXTBG_SRC);
    // Sprite spans x 112..144, y 88..120. Left of the x=128 split -> HIGH floor covers it;
    // right of the split -> LOW floor, the sprite shows through.
    assert!(
        is_red(px(&fb, 120, 104)),
        "high floor must cover the sprite left of split"
    );
    assert!(
        is_yellow(px(&fb, 136, 104)),
        "sprite must ride over the low floor right of split"
    );
    // Floor away from the sprite is red on both halves (same colour, different priority).
    assert!(is_red(px(&fb, 40, 104)), "left floor red");
    assert!(is_red(px(&fb, 210, 104)), "right floor red");
    // EXTBG off -> the sprite flat-overlays BOTH halves, so the left pixel is yellow.
    let off = EXTBG_SRC.replace("m7.extbg = true", "m7.extbg = false");
    let (flat, _) = render_demo(&off);
    assert!(
        is_yellow(px(&flat, 120, 104)),
        "EXTBG off should overlay the sprite everywhere"
    );
}

#[test]
fn extbg_demo_matches_golden_png() {
    assert!(Path::new(EXTBG_GOLDEN).exists());
    let (actual, _) = render_demo(EXTBG_SRC);
    let expected = decode_png(EXTBG_GOLDEN);
    assert_eq!(actual.len(), expected.len());
    assert_eq!(actual, expected, "extbg demo differs from golden PNG");
}

#[test]
#[ignore = "regenerates the committed Mode 7 EXTBG demo golden PNG"]
fn regen_golden_extbg() {
    let (fb, _) = render_demo(EXTBG_SRC);
    write_png(EXTBG_GOLDEN, &fb);
}

#[test]
fn direct_color_demo_bypasses_empty_cgram_into_smooth_gradient() {
    let (fb, e) = render_demo(DIRECT_SRC);
    // CGRAM is untouched (all zero) yet the floor is fully coloured -> direct-colour bypass.
    assert!(
        e.memory().cgram.iter().all(|&c| c == 0),
        "CGRAM must stay empty"
    );
    // Every pixel opaque (idx >= 64, never 0) -> full-screen gradient, no backdrop.
    assert!(
        fb.chunks_exact(4).all(|p| p[3] == 255),
        "gradient must fill the frame"
    );
    // Many distinct colours despite an empty palette.
    let colors = fb
        .chunks_exact(4)
        .map(|p| (p[0], p[1], p[2]))
        .collect::<std::collections::HashSet<_>>();
    assert!(
        colors.len() > 32,
        "expected a rich gradient, got {} colours",
        colors.len()
    );
    // Smooth axes: red rises left->right, green rises top->bottom.
    assert!(
        px(&fb, 248, 0)[0] > px(&fb, 0, 0)[0],
        "red should rise with x"
    );
    assert!(
        px(&fb, 0, 216)[1] > px(&fb, 0, 0)[1],
        "green should rise with y"
    );
}

#[test]
fn direct_color_demo_matches_golden_png() {
    assert!(Path::new(DIRECT_GOLDEN).exists());
    let (actual, _) = render_demo(DIRECT_SRC);
    let expected = decode_png(DIRECT_GOLDEN);
    assert_eq!(actual.len(), expected.len());
    assert_eq!(
        actual, expected,
        "direct-color demo differs from golden PNG"
    );
}

#[test]
#[ignore = "regenerates the committed direct-color demo golden PNG"]
fn regen_golden_direct_color() {
    let (fb, _) = render_demo(DIRECT_SRC);
    write_png(DIRECT_GOLDEN, &fb);
}

#[test]
fn multi_file_split_renders_identical_to_single_file() {
    let (single, _) = render_demo(OFFSET_SRC);

    let (helper, rest) = OFFSET_SRC.split_once("function frame").unwrap();
    let main = format!("function frame{rest}");
    let mut e = demo_engine_files(&[("util.lua", helper), ("main.lua", &main)]);
    let lt = e.frame(1.0, 60).unwrap();
    let multi = render_frame(&lt, e.memory());

    assert!(
        single == multi,
        "multi-file split must be framebuffer-identical"
    );
}

#[test]
fn dusk_parallax_multi_file_matches_golden_png() {
    let mut e = demo_engine_files(&[
        ("main.lua", DUSK_MAIN_SRC),
        ("palette.lua", DUSK_PALETTE_SRC),
    ]);
    let lt = e.frame(1.0, 60).unwrap();
    let multi = render_frame(&lt, e.memory());
    let expected = decode_png(DUSK_GOLDEN);
    assert_eq!(multi.len(), expected.len());
    assert!(
        multi == expected,
        "multi-file dusk must match the committed golden PNG"
    );
}

#[test]
fn dusk_parallax_multi_file_matches_single_file_concat() {
    let (single, _) = render_demo(&dusk_concat());
    let mut e = demo_engine_files(&[
        ("main.lua", DUSK_MAIN_SRC),
        ("palette.lua", DUSK_PALETTE_SRC),
    ]);
    let lt = e.frame(1.0, 60).unwrap();
    let multi = render_frame(&lt, e.memory());
    assert!(
        single == multi,
        "flagship split must be framebuffer-identical to its concatenation"
    );
}

// ── tilesheet-cavern (PPU-95) ───────────────────────────────────────────────
// The demo streams a 768px-wide Tiled-authored level across a 512px hardware
// tilemap while cycling animated tiles, so its two moving parts (camera, tile
// phase) have to be pinned independently to see either one.

/// A engine carrying ONLY the cavern demo's two sources. `demo_engine_files`
/// imports every demo's assets — including a 1024x1024 Mode 7 source — which
/// costs seconds per render, and the tests below render a dozen frames. The
/// golden test still goes through `render_demo` so the shipped path is covered.
fn cavern_engine(src: &str) -> LuaEngine {
    let mut e = LuaEngine::new();
    add_sheet(&mut e, "cavern_tiles", cavern_tiles(), 64, 24, 4);
    add_bg(
        &mut e,
        "cavern_back",
        cavern_back(),
        WIDTH as u32,
        HEIGHT as u32,
        4,
    );
    e.set_sources(&[("pokes.lua", EMPTY_POKES_SRC), ("main.lua", src)])
        .unwrap();
    e
}

/// The shipped demo with its camera pinned at `cam` px and its tile animation
/// held at `phase`, optionally with the assembled backdrop off the main screen —
/// so the frame becomes a pure function of whichever variable is under test.
fn cavern_pinned(cam: f64, phase: u32, backdrop: bool) -> Vec<u8> {
    cavern_pinned_src(CAVERN_SRC, cam, phase, backdrop)
}

/// True when `b` is `a` scrolled left by exactly `dx` px — the whole visible
/// frame, not a sample.
fn shifted_left_by(a: &[u8], b: &[u8], dx: usize) -> bool {
    (0..HEIGHT).all(|y| (0..WIDTH - dx).all(|x| px(a, x + dx, y) == px(b, x, y)))
}

// PPU-95: "Camera scrolls seamlessly past 512px: coarse streaming of the visible
// window into bg[n].map from the level table, fine scroll via the scroll
// registers mod 512."
#[test]
fn cavern_streams_the_level_across_the_512px_tilemap_wrap() {
    // The backdrop is off (its 1/3-rate parallax is not an integer px per step)
    // and the animation is frozen, so BG1's picture is a pure function of the
    // camera. Stepping the camera 8px must shift the frame exactly 8px — and the
    // step from 504 to 512 is the one that wraps the tilemap.
    let before = cavern_pinned(504.0, 0, false);
    let after = cavern_pinned(512.0, 0, false);
    assert!(
        shifted_left_by(&before, &after, 8),
        "streaming broke across the 512px tilemap wrap"
    );

    // The same step well away from the wrap, as a control on the comparison.
    assert!(
        shifted_left_by(
            &cavern_pinned(200.0, 0, false),
            &cavern_pinned(208.0, 0, false),
            8
        ),
        "streaming broke away from the wrap too — the test is measuring the wrong thing"
    );

    // The ring modulus is load-bearing: writing at the LEVEL column instead of
    // the TILEMAP column puts columns 64..95 into a different screen of the
    // 64x32 map, which the rasterizer never reads back at the same place.
    let mutant = CAVERN_SRC.replace(
        "local mcol = (cam_tile + s) % MAP_COLS",
        "local mcol = (cam_tile + s) % LEVEL_W",
    );
    assert_ne!(mutant, CAVERN_SRC, "ring-modulus mutation did not apply");
    let m_before = cavern_pinned_src(&mutant, 504.0, 0, false);
    let m_after = cavern_pinned_src(&mutant, 512.0, 0, false);
    assert!(
        !shifted_left_by(&m_before, &m_after, 8),
        "a wrong ring modulus still shifted cleanly — the seam test proves nothing"
    );
}

/// `cavern_pinned` against a possibly-mutated source. The pins are asserted to
/// have actually landed: a source edit that silently stopped matching would make
/// every test below vacuous.
fn cavern_pinned_src(src: &str, cam: f64, phase: u32, backdrop: bool) -> Vec<u8> {
    let cam_pin = format!("local cam = {cam}");
    let phase_pin = format!("local phase = {phase}");
    let mut s = src
        .replace("local cam = (t * SPEED) % LEVEL_PX", &cam_pin)
        .replace("local phase = floor(t * ANIM_HZ)", &phase_pin);
    assert!(s.contains(&cam_pin), "camera pin did not apply");
    assert!(s.contains(&phase_pin), "animation pin did not apply");
    if !backdrop {
        // The placement stays (its palette write is a byte-identical no-op by
        // the shared-palette invariant); the layer just leaves the main screen.
        let before = s.len();
        s = s.replace("bg[2].char_base = 0x2000", "screen.main.bg2 = false");
        assert_ne!(before, s.len(), "backdrop drop did not apply");
    }
    let mut e = cavern_engine(&s);
    let lt = e.frame(1.0, 60).unwrap();
    render_frame(&lt, e.memory())
}

// PPU-95: "Tile animation visible: map-entry `tile =` cycling between sheet
// variants as a function of frame time."
#[test]
fn cavern_cycles_its_lava_and_water_variants() {
    // Camera pinned, so the ONLY thing that can move is the map entry's `tile =`.
    let p0 = cavern_pinned(0.0, 0, true);
    let p1 = cavern_pinned(0.0, 1, true);
    let p4 = cavern_pinned(0.0, 4, true);
    assert_ne!(p0, p1, "the animation phase changed nothing on screen");
    assert_eq!(p0, p4, "lava/water is not a 4-variant cycle");

    // And it moves ONLY the animated materials: everything above the terrain
    // (screen tile rows 0..15, i.e. y < 128) is untouched between phases.
    for y in 0..128 {
        for x in 0..WIDTH {
            assert_eq!(
                px(&p0, x, y),
                px(&p1, x, y),
                "animation leaked outside the terrain at ({x},{y})"
            );
        }
    }
}

// PPU-95: "A second layer uses an assembled bg import with plain `scroll`
// scrolling, on screen at the same time."
#[test]
fn cavern_scrolls_an_assembled_import_beside_the_tilesheet_layer() {
    // Both kinds really are on screen: dropping the backdrop changes the frame,
    // and what it changes is the band above the terrain.
    let both = cavern_pinned(0.0, 0, true);
    let sheet_only = cavern_pinned(0.0, 0, false);
    assert_ne!(both, sheet_only, "the assembled backdrop is not on screen");
    assert_ne!(
        px(&both, 4, 40),
        px(&sheet_only, 4, 40),
        "the sky band is not coming from the assembled import"
    );

    // The backdrop scrolls at exactly 1/3 the camera rate through the plain
    // scroll register: a 48px camera step moves the pure-backdrop band 16px,
    // while the sheet layer moves the full 48.
    let a = cavern_pinned(0.0, 0, true);
    let b = cavern_pinned(48.0, 0, true);
    for y in 0..128 {
        for x in 0..WIDTH - 16 {
            assert_eq!(
                px(&a, x + 16, y),
                px(&b, x, y),
                "backdrop parallax is not 1/3 the camera rate at ({x},{y})"
            );
        }
    }
    assert!(
        shifted_left_by(
            &cavern_pinned(0.0, 0, false),
            &cavern_pinned(48.0, 0, false),
            48
        ),
        "the tilesheet layer did not scroll at the full camera rate"
    );
}

// PPU-95: "Map data table matches Tiled's Lua export shape, converted with a
// `gid - 1` adapter." Checked by colour so it is independent of how the demo
// indexes anything.
#[test]
fn cavern_gid_adapter_puts_the_authored_level_on_screen() {
    let fb = cavern_pinned(0.0, 0, true);
    // At camera 0 the lava pit (level columns 22..30) sits at screen x 176..247,
    // its surface on screen tile row 26 -> y 208..215.
    let lava = px(&fb, 200, 212);
    assert!(
        lava[0] > 0xa0 && lava[2] < 0x70,
        "expected lava at (200,212), got {lava:?}"
    );
    // A plain rock column at the same row is grey — r, g and b within 0x20.
    let rock = px(&fb, 8, 212);
    let (lo, hi) = (
        rock[0].min(rock[1]).min(rock[2]),
        rock[0].max(rock[1]).max(rock[2]),
    );
    assert!(
        hi - lo < 0x20,
        "expected grey rock at (8,212), got {rock:?}"
    );
    // Tiled's empty cell (gid 0) maps onto sheet cell 0, which is blank. Level
    // rows 0..2 are entirely gid 0, so screen y 128..151 is a band the streaming
    // loop DOES write, every entry of it through gid_to_tile(0) — the band above
    // (y < 128) is never written at all and would stay transparent whatever the
    // adapter did, which is why it is the wrong place to check this.
    let no_sheet = cavern_pinned_src(
        &CAVERN_SRC.replace("bg[1].char_base = 0x1000", "screen.main.bg1 = false"),
        0.0,
        0,
        true,
    );
    for y in 128..152 {
        for x in 0..WIDTH {
            assert_eq!(
                px(&fb, x, y),
                px(&no_sheet, x, y),
                "an empty Tiled cell is not drawing the blank sheet cell at ({x},{y})"
            );
        }
    }
    // ...and the sheet layer is genuinely doing something lower down.
    assert_ne!(px(&fb, 8, 212), px(&no_sheet, 8, 212));
}

// PPU-95: the sheet must stay under the 1024-char ceiling. Past it a cells[k]
// with tile >= 1024 masks to 10 bits and renders cell k & 0x3ff — a wrong
// picture, not a blank one — and a successfully bound sheet emits no
// ImportBudget, so the inspector cannot warn. The demo checks its own sheet.
#[test]
fn cavern_sheet_stays_under_the_char_ceiling() {
    let opts = ConvertOptions {
        bit_depth: Some(4),
        ..Default::default()
    };
    let (_, meta) = convert_source(SourceKind::Sheet, &opts, &cavern_tiles(), 64, 24).unwrap();
    let ppu_core::SourceReport::Sheet { report } = &meta.report else {
        panic!("cavern_tiles did not import as a sheet: {:?}", meta.report);
    };
    assert_eq!(report.unique_tiles, 24, "sheet char count changed");
    assert!(
        report.overflows.is_empty(),
        "sheet overflowed: {:?}",
        report.overflows
    );
    assert_eq!(meta.cells.as_ref().map(|c| c.len()), Some(24));
}

// PPU-95: both sources are drawn from ONE master palette because both dma calls
// land their palettes at CGRAM 0 and the backdrop is placed after the sheet. If
// they ever diverge, the backdrop import silently recolours the tilesheet layer.
#[test]
fn cavern_backdrop_import_does_not_recolour_the_tilesheet_layer() {
    let mut both = cavern_engine(CAVERN_SRC);
    both.frame(1.0, 60).unwrap();
    let with_backdrop = both.memory().cgram;

    let sheet_only =
        CAVERN_SRC.replace(r#"dma("cavern_back", { char = 0x2000, map = 0x0800 })"#, "");
    assert_ne!(
        sheet_only, CAVERN_SRC,
        "backdrop placement removal did not apply"
    );
    let mut only = cavern_engine(&sheet_only);
    only.frame(1.0, 60).unwrap();

    assert_eq!(
        with_backdrop,
        only.memory().cgram,
        "the assembled backdrop's palette overwrote the tilesheet's"
    );
}

#[test]
fn tilesheet_cavern_demo_matches_golden_png() {
    assert!(Path::new(CAVERN_GOLDEN).exists());
    let (actual, _) = render_demo(CAVERN_SRC);
    let expected = decode_png(CAVERN_GOLDEN);
    assert_eq!(actual.len(), expected.len());
    assert_eq!(actual, expected, "tilesheet-cavern differs from golden PNG");
}

#[test]
#[ignore = "regenerates the committed tilesheet-cavern demo golden PNG"]
fn regen_golden_tilesheet_cavern() {
    let (fb, _) = render_demo(CAVERN_SRC);
    write_png(CAVERN_GOLDEN, &fb);
}
