//! cavern-camera (L1 tutorial toy 5/10): the tilesheet workflow, hand-authored.
//! Rust mirror of web/src/studio/demos/tutorials/cavernCamera.ts — MAIN_SRC is
//! byte-identical to the TS template literal (a plain string, not raw: the
//! level rows put arbitrarily long `"###...` runs in the Lua, which no raw
//! delimiter survives) and tiles() reproduces the TS generator pixel-exact.
mod common;

use common::{add_sheet, decode_png, engine_with, px, write_png};
use ppu_core::{render_frame, LuaEngine, HEIGHT, WIDTH};
use std::path::Path;

const GOLDEN: &str = "tests/fixtures/golden_tutorial_cavern_camera.png";

// ── Lua (byte-identical to cavernCamera.ts MAIN_SRC) ────────────────────────

const MAIN_SRC: &str = "-- ppu.toys :: cavern-camera (lesson 5 of 10 — build a LEVEL out of tiles)
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
local sheet = dma(\"cave_tiles\", { char = 0x1000 })
--
-- ==== TYPE YOUR OWN LEVEL HERE ==============================================
-- One string per row, one character per 8x8 tile:
--   .  air (blank sheet cell 0)     #  rock          %  mossy rock
--   =  wooden platform              *  crystal       ~  lava (animates itself)
-- Keep every row the same length. Up to 64 characters wide fits the 64x32
-- hardware tilemap whole, so the camera pans with NO streaming at all.
local LEVEL = {
  \"################################################\",
  \"#######....#########....############....########\",
  \"###......*....####........####.........*....####\",
  \"##..............##..........##................##\",
  \"#........====..........................====....#\",
  \"#...............................*..............#\",
  \"#....%%%..............====.............%%%.....#\",
  \"#....###..........................*....###.....#\",
  \"#....###....%%%%.......%%%.....%%......###.....#\",
  \"#....###....####.......###.....##......###.....#\",
  \"#....###....####.......###.....##......###..%%.#\",
  \"#%%%%###%%%%####%~~~~%%###%%%%%##%~~~~%###%%##%#\",
  \"################################################\",
  \"################################################\",
}

-- The legend: which sheet cell each character draws. A sheet reserves NO blank
-- tile — cell 0 is blank only because the artist left cell 0 empty on purpose.
local TILES = { [\".\"] = 0, [\"#\"] = 1, [\"%\"] = 2, [\"=\"] = 3, [\"~\"] = 4, [\"*\"] = 7 }
local LAVA_FIRST, LAVA_FRAMES = 4, 3 -- lava's variants are consecutive cells 4..6
local LEVEL_W = #LEVEL[1]            -- tiles across (48 here; 64 is the max)
local LEVEL_PX = LEVEL_W * 8         -- 384 px of level over a 256 px screen
local MAP_TOP = 7                    -- screen tile row where LEVEL row 1 lands
local SPEED = 48                     -- camera pixels per second
local ANIM_HZ = 6                    -- lava steps per second

function frame(t, f)
  apply_pokes()
  mode = 1; brightness = 15
  screen.main.bg1 = true    -- the level, alone on the main screen (TM)

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
      if ch == \"~\" then tile = LAVA_FIRST + phase % LAVA_FRAMES end
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
";

// ── the tilesheet (mirrors cavernCamera.ts PAL / CELLS / tiles()) ───────────

/// One 4bpp sub-palette; index 0 is the transparent lane, never placed.
const PAL: [(u8, u8, u8); 13] = [
    (0x00, 0x00, 0x00), // 0 = transparent, never placed
    (0x10, 0x18, 0x28), // 1 rock crack (deep shadow)
    (0x38, 0x38, 0x40), // 2 rock dark
    (0x58, 0x58, 0x68), // 3 rock mid
    (0x90, 0x90, 0x98), // 4 rock light
    (0x58, 0xa8, 0x48), // 5 moss
    (0x60, 0x48, 0x30), // 6 wood dark
    (0x98, 0x70, 0x48), // 7 wood light
    (0xb8, 0x38, 0x10), // 8 lava dark
    (0xf0, 0x70, 0x18), // 9 lava orange
    (0xff, 0xc8, 0x50), // a lava bright
    (0x48, 0x70, 0xa8), // b crystal mid
    (0x78, 0xc0, 0xe0), // c crystal light
];

/// 8 8x8 cells in sheet order; 64 hex indices each, '0' = transparent.
/// Cell 0 is blank on purpose — a sheet reserves no blank tile.
const CELLS: [&str; 8] = [
    // 0 blank (air — the author-reserved empty tile)
    "
      00000000
      00000000
      00000000
      00000000
      00000000
      00000000
      00000000
      00000000
    ",
    // 1 rock fill
    "
      33343233
      34133332
      33233433
      33332313
      23433233
      13333423
      33233333
      43333213
    ",
    // 2 mossy rock (moss cap over the same rock)
    "
      55555555
      55355535
      33343233
      34133332
      33233433
      33332313
      23433233
      13333423
    ",
    // 3 wooden platform (plank + support stem)
    "
      77777777
      67676767
      66666666
      06677660
      00066000
      00066000
      00000000
      00000000
    ",
    // 4 lava frame 0
    "
      9a99a9a9
      99999999
      89988998
      98898889
      88888888
      88888888
      88888888
      88888888
    ",
    // 5 lava frame 1
    "
      a99a99aa
      99999999
      98899889
      88988898
      88888888
      88888888
      88888888
      88888888
    ",
    // 6 lava frame 2
    "
      99aa9a99
      99999999
      88998998
      89888988
      88888888
      88888888
      88888888
      88888888
    ",
    // 7 crystal
    "
      000cc000
      00cbbc00
      00cbbc00
      0cbbbbc0
      0cbbbbc0
      00cbbc00
      000cc000
      00000000
    ",
];

/// RGBA generator matching cavernCamera.ts tiles() pixel-exact: 8 cells of
/// 8x8 in one 64x8 row, index 0 left at alpha 0 so the backdrop shows.
fn tiles() -> Vec<u8> {
    let cols = 8usize;
    let (w, h) = (cols * 8, (CELLS.len() / cols) * 8);
    let mut buf = vec![0u8; w * h * 4];
    for (n, cell) in CELLS.iter().enumerate() {
        let s: Vec<u32> = cell
            .chars()
            .filter(|c| !c.is_whitespace())
            .map(|c| c.to_digit(16).unwrap())
            .collect();
        assert_eq!(s.len(), 64, "cavern-camera cell {n} is not 8x8");
        let (ox, oy) = ((n % cols) * 8, (n / cols) * 8);
        for y in 0..8 {
            for x in 0..8 {
                let idx = s[y * 8 + x];
                if idx == 0 {
                    continue; // transparent
                }
                let (r, g, b) = PAL[idx as usize];
                let i = ((oy + y) * w + ox + x) * 4;
                buf[i..i + 4].copy_from_slice(&[r, g, b, 255]);
            }
        }
    }
    buf
}

// ── harness ─────────────────────────────────────────────────────────────────

fn engine(src: &str) -> LuaEngine {
    engine_with(
        &mut |e| add_sheet(e, "cave_tiles", tiles(), 64, 8, 4),
        &[("main.lua", src)],
    )
}

fn render(src: &str, t: f64) -> Vec<u8> {
    let mut e = engine(src);
    let lt = e.frame(t, 60).unwrap();
    render_frame(&lt, e.memory())
}

/// True when `b` is `a` scrolled left by exactly `dx` px, over the whole frame.
fn shifted_left_by(a: &[u8], b: &[u8], dx: usize) -> bool {
    (0..HEIGHT).all(|y| (0..WIDTH - dx).all(|x| px(a, x + dx, y) == px(b, x, y)))
}

/// MAIN_SRC with its animation phase pinned, camera untouched.
fn phase_pinned(phase: u32) -> String {
    let pin = format!("local phase = {phase}");
    let s = MAIN_SRC.replace("local phase = floor(t * ANIM_HZ)", &pin);
    assert!(s.contains(&pin), "phase pin did not apply");
    s
}

// At t=1.0 the camera sits at 48 px (SPEED=48, ping-pong range 128), which is
// 6 whole tiles: screen tile column s shows LEVEL column s + 6, tile-aligned.
// Screen y for LEVEL row r (1-based) is (MAP_TOP + r - 1) * 8 = (6 + r) * 8.

// The hand-authored strings actually land in the tilemap: rock where the level
// says '#', backdrop where it says '.', lava colour over '~'.
#[test]
fn level_strings_land_on_the_map() {
    let fb = render(MAIN_SRC, 1.0);

    // LEVEL row 1 is solid '#': grey rock at screen y 56..63.
    let rock = px(&fb, 20, 60);
    let (lo, hi) = (
        rock[0].min(rock[1]).min(rock[2]),
        rock[0].max(rock[1]).max(rock[2]),
    );
    assert!(
        hi - lo < 0x20,
        "expected grey rock at (20,60), got {rock:?}"
    );
    assert!(hi > 0x20, "rock at (20,60) is black, not rock: {rock:?}");

    // LEVEL row 6 col 11 ('.') at screen x 36, y 100..107: pure backdrop, the
    // same colour as y=20 — a scanline the map never covers at all.
    let air = px(&fb, 36, 104);
    assert_eq!(air, px(&fb, 36, 20), "air cell is not showing the backdrop");
    assert!(
        air[2] > air[0],
        "backdrop is not the cave-air blue: {air:?}"
    );
    assert_ne!(air, rock, "rock and air render identically");

    // LEVEL row 12 cols 18..21 are '~': lava at screen x 100 (col 18 -> screen
    // tile 12), y 144 (the bright surface row of the lava cell).
    let lava = px(&fb, 100, 144);
    assert!(
        lava[0] > 0xa0 && lava[2] < 0x70,
        "expected lava at (100,144), got {lava:?}"
    );
}

// The camera is the scroll register alone: t=0 -> 0 px, t=1 -> 48 px, and the
// lava phase is 0 at both (0 and 6 mod 3), so the t=1 frame must be the t=0
// frame shifted left exactly 48 px. Also proves scroll is nonzero at t=1.
#[test]
fn camera_pans_the_level_between_frames() {
    let f0 = render(MAIN_SRC, 0.0);
    let f1 = render(MAIN_SRC, 1.0);
    assert_ne!(f0, f1, "the camera did not move between t=0 and t=1");
    assert!(
        shifted_left_by(&f0, &f1, 48),
        "t=1 is not the t=0 frame panned 48 px"
    );
}

// The '~' entries cycle through the three consecutive lava cells; everything
// else is untouched by the phase.
#[test]
fn lava_cycles_its_three_variants() {
    let p0 = render(&phase_pinned(0), 1.0);
    let p1 = render(&phase_pinned(1), 1.0);
    let p3 = render(&phase_pinned(3), 1.0);
    assert_ne!(p0, p1, "the animation phase changed nothing on screen");
    assert_eq!(p0, p3, "lava is not a 3-variant cycle");

    // Only the lava rows move: everything above LEVEL row 12 (y < 144) is
    // identical between phases.
    for y in 0..144 {
        for x in 0..WIDTH {
            assert_eq!(
                px(&p0, x, y),
                px(&p1, x, y),
                "phase moved a non-lava pixel at ({x},{y})"
            );
        }
    }
}

#[test]
fn tutorial_cavern_camera_matches_golden_png() {
    assert!(
        Path::new(GOLDEN).exists(),
        "golden missing — run the regen test"
    );
    let actual = render(MAIN_SRC, 1.0);
    let expected = decode_png(GOLDEN);
    assert_eq!(actual.len(), expected.len());
    assert_eq!(actual, expected, "cavern-camera differs from golden PNG");
}

#[test]
#[ignore = "regenerates the committed cavern-camera tutorial golden PNG"]
fn regen_golden_tutorial_cavern_camera() {
    write_png(GOLDEN, &render(MAIN_SRC, 1.0));
}
