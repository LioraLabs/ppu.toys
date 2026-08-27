/** cavern-camera — L1 tutorial toy 5 of 10, the fundamentals-arc capstone.
 *  Teaches the tilesheet workflow: dma a "sheet" source in, author a level map
 *  by hand in Lua, write bg[1].map[col][row] entries, pan a camera. The
 *  teaching-sized sibling of the tilesheet-cavern flagship demo (which streams
 *  a Tiled export through a ring buffer — pointed at from the Lua comments).
 *  Rust mirror: crates/ppu-core/tests/tutorial_cavern_camera.rs. */
import { demo, type Demo, type DemoAsset } from "../kit";

// One 4bpp sub-palette (15 colours max; 12 used). Index 0 is the transparent
// lane — never placed as a pixel. Channels are multiples of 8 (the rgb15 grid)
// so nothing collapses when 8-bit colour reduces to 5-bit.
const PAL = [
  0x000000, // 0 = transparent, never placed
  0x101828, // 1 rock crack (deep shadow)
  0x383840, // 2 rock dark
  0x585868, // 3 rock mid
  0x909098, // 4 rock light
  0x58a848, // 5 moss
  0x604830, // 6 wood dark
  0x987048, // 7 wood light
  0xb83810, // 8 lava dark
  0xf07018, // 9 lava orange
  0xffc850, // a lava bright
  0x4870a8, // b crystal mid
  0x78c0e0, // c crystal light
];

/** 8 8x8 cells in SHEET order, so cell N is what `tile = N` draws. One
 *  template literal per cell: 64 hex palette indices, whitespace stripped,
 *  '0' = transparent. Cell 0 is blank on purpose — a sheet reserves no blank
 *  tile, so the author leaves one for the level's air. */
const CELLS = [
  // 0 blank (air — the author-reserved empty tile)
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
  // 1 rock fill
  `
    33343233
    34133332
    33233433
    33332313
    23433233
    13333423
    33233333
    43333213
  `,
  // 2 mossy rock (moss cap over the same rock)
  `
    55555555
    55355535
    33343233
    34133332
    33233433
    33332313
    23433233
    13333423
  `,
  // 3 wooden platform (plank + support stem)
  `
    77777777
    67676767
    66666666
    06677660
    00066000
    00066000
    00000000
    00000000
  `,
  // 4 lava frame 0
  `
    9a99a9a9
    99999999
    89988998
    98898889
    88888888
    88888888
    88888888
    88888888
  `,
  // 5 lava frame 1
  `
    a99a99aa
    99999999
    98899889
    88988898
    88888888
    88888888
    88888888
    88888888
  `,
  // 6 lava frame 2
  `
    99aa9a99
    99999999
    88998998
    89888988
    88888888
    88888888
    88888888
    88888888
  `,
  // 7 crystal
  `
    000cc000
    00cbbc00
    00cbbc00
    0cbbbbc0
    0cbbbbc0
    00cbbc00
    000cc000
    00000000
  `,
].map((c) => c.replace(/\s/g, ""));

function tiles(): DemoAsset {
  const cols = 8;
  const w = cols * 8,
    h = (CELLS.length / cols) * 8;
  const data = new Uint8ClampedArray(w * h * 4);
  CELLS.forEach((cell, n) => {
    const ox = (n % cols) * 8,
      oy = Math.floor(n / cols) * 8;
    for (let y = 0; y < 8; y++) {
      for (let x = 0; x < 8; x++) {
        const idx = parseInt(cell[y * 8 + x], 16);
        if (!idx) continue; // palette index 0 -> transparent, the backdrop shows
        const c = PAL[idx];
        const i = ((oy + y) * w + ox + x) * 4;
        data[i] = c >> 16;
        data[i + 1] = (c >> 8) & 0xff;
        data[i + 2] = c & 0xff;
        data[i + 3] = 255;
      }
    }
  });
  return {
    id: "cave_tiles",
    width: w,
    height: h,
    data,
    kind: "sheet",
    options: { bit_depth: 4 },
  };
}

const MAIN_SRC = `-- ppu.toys :: cavern-camera (lesson 5 of 10 — build a LEVEL out of tiles)
--
-- You know layers, scroll and sprites (first-light .. sprite-parade). This is
-- the capstone of the fundamentals: the tilesheet workflow, in three steps.
--   1. dma a 'sheet' source in        -- chars land in sheet order: tile N = cell N
--   2. set the map geometry YOURSELF  -- a sheet is chars + palette, NO map
--   3. write bg[1].map[col][row]      -- the tilemap IS your level
--
-- Step 1, the setup stage (parallax-skyline, lesson 2, is the full dma
-- story): a sheet carries no tilemap — where tiles GO is this whole lesson —
-- so the only address it takes is where its chars land.
local sheet = dma("cave_tiles", { char = 0x1000 })
--
-- ==== TYPE YOUR OWN LEVEL HERE ==============================================
-- One string per row, one character per 8x8 tile:
--   .  air (blank sheet cell 0)     #  rock          %  mossy rock
--   =  wooden platform              *  crystal       ~  lava (animates itself)
-- Keep every row the same length. Up to 64 characters wide fits the 64x32
-- hardware tilemap whole, so the camera pans with NO streaming at all.
local LEVEL = {
  "################################################",
  "#######....#########....############....########",
  "###......*....####........####.........*....####",
  "##..............##..........##................##",
  "#........====..........................====....#",
  "#...............................*..............#",
  "#....%%%..............====.............%%%.....#",
  "#....###..........................*....###.....#",
  "#....###....%%%%.......%%%.....%%......###.....#",
  "#....###....####.......###.....##......###.....#",
  "#....###....####.......###.....##......###..%%.#",
  "#%%%%###%%%%####%~~~~%%###%%%%%##%~~~~%###%%##%#",
  "################################################",
  "################################################",
}

-- The legend: which sheet cell each character draws. A sheet reserves NO blank
-- tile — cell 0 is blank only because the artist left cell 0 empty on purpose.
local TILES = { ["."] = 0, ["#"] = 1, ["%"] = 2, ["="] = 3, ["~"] = 4, ["*"] = 7 }
local LAVA_FIRST, LAVA_FRAMES = 4, 3 -- lava's variants are consecutive cells 4..6
local LEVEL_W = #LEVEL[1]            -- tiles across (48 here; 64 is the max)
local LEVEL_PX = LEVEL_W * 8         -- 384 px of level over a 256 px screen
local MAP_TOP = 7                    -- screen tile row where LEVEL row 1 lands
local SPEED = 48                     -- camera pixels per second
local ANIM_HZ = 6                    -- lava steps per second

function frame(t, f)
  apply_pokes()
  mode = 1; brightness = 15

  -- Step 2: geometry is YOURS. dma placed chars and palette, nothing more;
  -- every register below is you telling the chip how to read the tilemap
  -- you are about to write.
  bg[1].char_base = sheet.char  -- point BG1 at the chars from the setup stage
  bg[1].map_base = 0x0000   -- the tilemap starts at VRAM word 0
  bg[1].screen_size = 1     -- 64x32 tiles = 512x256 px; the whole level fits

  -- The backdrop colour is what shows through every '.' — the cave air.
  cgram[0] = rgb(16, 24, 40)

  -- Step 3: parse the strings into map entries. VRAM is zeroed every frame
  -- before imports, so this loop runs EVERY frame — and that is also the
  -- animation mechanism: the same '~' gets a different tile as phase ticks.
  local phase = floor(t * ANIM_HZ)
  for col = 0, LEVEL_W - 1 do
    if bg[1].map[col] == nil then bg[1].map[col] = {} end
    for row = 1, #LEVEL do
      local ch = string.sub(LEVEL[row], col + 1, col + 1)
      local tile = TILES[ch] or 0
      if ch == "~" then tile = LAVA_FIRST + phase % LAVA_FRAMES end
      bg[1].map[col][MAP_TOP + row - 1] = { tile = tile, pal = 0 }
    end
  end

  -- The camera. The whole level is already in the tilemap, so the scroll
  -- register alone IS the camera: ping-pong over the level's spare width
  -- (LEVEL_PX - 256) and both ends of your level get their moment on screen.
  local range = LEVEL_PX - 256
  local m = (t * SPEED) % (range * 2)
  if m > range then m = range * 2 - m end
  bg[1].scroll.x = m
end

-- A bigger world than 64x32 tiles can't sit in the tilemap whole. The
-- tilesheet-cavern toy streams a 96-tile-wide Tiled-authored level through
-- this same tilemap with a ring buffer — fork it when your level outgrows
-- 64 columns.
--
-- Try:
--   * type your own rows into LEVEL (same length each; '~' animates for free)
--   * change SPEED, or swap the ping-pong for a wobble: 64 + sin(t) * 64
--   * add a tile type: draw a new 8x8 cell in the sheet, add one TILES entry
`;

export const cavernCamera: Demo = demo(
  "cavern-camera",
  "cavern-camera",
  [{ name: "main.lua", source: MAIN_SRC }],
  [tiles()],
);
