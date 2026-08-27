//! sprite-parade tutorial toy (4/10): Rust mirror of
//! web/src/studio/demos/tutorials/spriteParade.ts — the Lua is verbatim and the
//! asset generators are byte-identical, so the golden PNG here is evidence
//! about the toy the studio actually ships. Edit both files together.
mod common;

use ppu_core::{
    convert_source, render_frame, rgb15, unpack_rgb15, ConvertOptions, ImportBudget, LuaEngine,
    SourceKind,
};
use std::path::Path;

const GOLDEN: &str = "tests/fixtures/golden_tutorial_sprite_parade.png";

// ── the Lua (verbatim from spriteParade.ts MAIN_SRC) ─────────────────────────
const MAIN_SRC: &str = r#"-- ppu.toys :: sprite-parade — tutorial 4/10: sprites (OBJ) and OAM
--
-- The SNES draws sprites out of OAM: 128 slots, each an x/y, a tile number,
-- a palette, a priority and two flip bits. Here that table is obj[0..127].
-- What this toy walks through, in reading order:
--   1. BINDING A SHEET   obj.sheet + obj.char_base put your art in OBJ VRAM
--   2. SIZES             obj.size_sel picks ONE small/large pair per frame;
--                        obj[i].large flips a single sprite to the large size
--   3. PLACING           obj[i].x/y/tile/pal/on — that is a sprite on screen
--   4. FLIPPING          obj[i].flip_x mirrors (the marcher walking back)
--   5. PALETTES          OBJ palettes live at CGRAM 128+, 16 entries each
--   6. PRIORITY          obj[i].prio 0..3 interleaves with the BG planes —
--                        watch the fence: prio 0 marches BEHIND it, prio 3 over
-- (tutorial 1 first-light covers frame(t,f); 2 parallax-skyline covers BG
--  layers; 5 cavern-camera covers tilesheets; 10 sprite-limits pushes OAM
--  until the hardware starts dropping sprites.)

SPEED = 32           -- parade pace, pixels per second

function frame(t, f)
  apply_pokes()
  mode = 1; brightness = 15
  cgram[0] = rgb(96, 64, 128)   -- backdrop = dusk sky (CGRAM entry 0)

  -- The street + picket fence: one assembled BG import. Its palette lands at
  -- CGRAM 0; sprites never touch it — OBJ palettes live in the 128+ half.
  bg[1].source = "street"
  -- Power-on defaults turn every layer on, and an unbound layer rasterizes
  -- whatever VRAM holds. Keep BG1 + OBJ, drop the rest (screen.main = TM).
  screen.main.bg2 = false; screen.main.bg3 = false; screen.main.bg4 = false

  -- 1. BIND THE SHEET. The cell_size-8 OBJ importer reserves tile 0 as blank
  -- and numbers each new unique 8x8 cell in order; this sheet leaves cell 0
  -- blank ON PURPOSE so sheet cell N = OBJ tile N from there on.
  obj.char_base = 0x6000        -- OBJ chars live apart from the BG chars
  obj.sheet = "parade"

  -- 2. SIZES. OBSEL holds one pair for the whole frame; each sprite picks
  -- its half of the pair with obj[i].large:
  --   size_sel  0: 8x8/16x16   1: 8/32   2: 8/64   3: 16/32   4: 16/64
  --             5: 32/64   6: 16x32/32x64   7: 16x32/32x32  <- 6+7 NON-square
  obj.size_sel = 0              -- small = 8x8, large = 16x16
  -- A large 16x16 sprite fetches tiles base, base+1, base+16, base+17: the
  -- OBJ name table is 16 tiles wide. The sheet is 16 CELLS wide for exactly
  -- that reason — a 2x2 block of cells in the image is one large sprite.

  -- 5. PALETTES. OBJ palette p starts at CGRAM 128 + p*16 (entry 0 of each
  -- is transparent). The import filled palette 0; palette 1 is ours — 15
  -- shades of gold, so one marcher parades as a statue of itself.
  for i = 1, 15 do cgram[128 + 16 + i] = rgb(96 + i * 10, 72 + i * 9, 8 + i * 3) end

  local function march(x0)      -- drift right, wrapping just off both edges
    return ((x0 + t * SPEED) % 272) - 16
  end
  local step = floor(t * 6)     -- walk cycle: poses 1 and 2 alternate
  local function pose(i) return 1 + (step + i) % 2 end
  local function bob(i) return -abs(sin(t * 6 + i)) * 2 end

  -- 3. PLACE THE PARADE. Feet on the kerb at y=180, so small 8x8 marchers
  -- stand at y 172 and large 16x16 ones at y 164. obj[i].on = true or the
  -- slot stays empty.
  obj[0].tile = 4; obj[0].large = true    -- flag-bearer: ONE tile number (4)
  obj[0].x = march(160); obj[0].y = 164 + bob(0)   -- addresses the 2x2 block
  obj[0].prio = 2; obj[0].on = true
  obj[1].tile = 6; obj[1].large = true    -- big robot, the other 2x2 block
  obj[1].x = march(32); obj[1].y = 164 + bob(1)
  obj[1].prio = 2; obj[1].on = true
  obj[2].tile = pose(0); obj[2].x = march(64)      -- two small robots,
  obj[2].y = 172 + bob(2); obj[2].prio = 2; obj[2].on = true
  obj[3].tile = pose(1); obj[3].x = march(188)     -- walking out of phase
  obj[3].y = 172 + bob(3); obj[3].prio = 2; obj[3].on = true

  -- 4. FLIP: same tiles, flip_x = true, marching the other way.
  obj[4].tile = pose(0); obj[4].flip_x = true
  obj[4].x = ((280 - t * SPEED) % 272) - 16
  obj[4].y = 172 + bob(4); obj[4].prio = 2; obj[4].on = true

  obj[5].tile = pose(1); obj[5].pal = 1   -- the gold marcher: same tiles,
  obj[5].x = march(-4); obj[5].y = 172 + bob(5)    -- palette 1 from above
  obj[5].prio = 2; obj[5].on = true
  obj[6].tile = 3; obj[6].x = march(16)   -- drummer
  obj[6].y = 172 + bob(6); obj[6].prio = 2; obj[6].on = true

  -- 6. PRIORITY. In mode 1 a prio-0 sprite sits UNDER the low-priority BG
  -- planes (an assembled import's tilemap priority bit is 0); prio 1+ sit
  -- over them, prio 3 over everything. Same tiles, one number apart:
  obj[7].tile = pose(0); obj[7].prio = 0  -- BEHIND the fence — slivers of it
  obj[7].x = march(104); obj[7].y = 172   -- show through the picket gaps
  obj[7].on = true
  obj[8].tile = pose(1); obj[8].prio = 3  -- in FRONT of the same fence
  obj[8].x = march(128); obj[8].y = 172
  obj[8].on = true

  -- Set dressing: bunting on slots 9..16 (tiles 8..15), drifting confetti,
  -- one escaped balloon. All plain small sprites.
  for k = 0, 7 do
    obj[9 + k].tile = 8 + k
    obj[9 + k].x = 12 + k * 32; obj[9 + k].y = 4
    obj[9 + k].prio = 3; obj[9 + k].on = true
  end
  for k = 0, 2 do
    obj[17 + k].tile = 17 + k
    obj[17 + k].x = 40 + k * 70; obj[17 + k].y = 36 + sin(t * 2 + k) * 12
    obj[17 + k].prio = 3; obj[17 + k].on = true
  end
  obj[20].tile = 16
  obj[20].x = 30 + sin(t * 3) * 3; obj[20].y = 132 - t * 16
  obj[20].prio = 2; obj[20].on = true

  -- Try: set obj.size_sel = 3 and watch every small marcher double in size;
  -- make the flag-bearer bob twice as hard; give obj[8] pal = 1 and prio = 0
  -- so the gold statue is the one stuck behind the fence.
end
"#;

// ── the parade sheet (mirrors PARADE_PAL / PARADE_CELLS) ─────────────────────
const PARADE_PAL: [(u8, u8, u8); 15] = [
    (0x00, 0x00, 0x00), //  0 transparent, never placed
    (0x10, 0x18, 0x20), //  1 outline
    (0x40, 0x50, 0x68), //  2 steel
    (0x98, 0xa8, 0xc0), //  3 light steel
    (0xf0, 0xf0, 0xf0), //  4 white
    (0xd0, 0x38, 0x30), //  5 red
    (0xf8, 0xa8, 0x00), //  6 gold
    (0x38, 0x78, 0xd0), //  7 blue
    (0x48, 0xa0, 0x48), //  8 green
    (0xe8, 0x70, 0xa8), //  9 pink
    (0x80, 0x50, 0x30), // 10 brown
    (0xf8, 0xd8, 0xb0), // 11 skin
    (0x68, 0x58, 0xb8), // 12 purple
    (0xf8, 0x68, 0x00), // 13 orange
    (0x78, 0xe0, 0xe8), // 14 cyan visor
];

const TRI: &str = "
  11111111
  0CCCCCC0
  0CCCCCC0
  00CCCC00
  00CCCC00
  000CC000
  000CC000
  00000000
";
const SWAL: &str = "
  11111111
  0CCCCCC0
  0CCCCCC0
  0CCCCCC0
  0CC00CC0
  0C0000C0
  00000000
  00000000
";

fn parade_cells() -> Vec<String> {
    let fixed: [&str; 24] = [
        //  0 blank
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
        //  1 robot pose A
        "
          00060000
          00111100
          001ee200
          00122200
          02233220
          00233200
          00200200
          02200220
        ",
        //  2 robot pose B
        "
          00060000
          00111100
          001ee200
          00122200
          00233200
          00233200
          00022000
          00022000
        ",
        //  3 drummer
        "
          00060000
          00777700
          001bb100
          07777770
          04444440
          0a5555a0
          0aaaaaa0
          01000100
        ",
        //  4 flag-bearer top-left
        "
          00000000
          00555555
          05555555
          05555665
          05555555
          00555555
          00005555
          00004444
        ",
        //  5 flag-bearer top-right
        "
          00060000
          555a0000
          555a0000
          555a0000
          555a0000
          555a0000
          555a0000
          000a0000
        ",
        //  6 big robot top-left
        "
          00000000
          00011111
          00012222
          0001ee22
          00012222
          00011222
          00001111
          00000022
        ",
        //  7 big robot top-right
        "
          00600000
          11111000
          22222100
          ee221000
          22222100
          22221100
          11110000
          22000000
        ",
        // 8..15 filled below (bunting)
        "",
        "",
        "",
        "",
        "",
        "",
        "",
        "",
        // 16 balloon
        "
          00999900
          09499990
          09999990
          09999990
          00999900
          00090000
          00010000
          00001000
        ",
        // 17 confetti A
        "
          05500066
          05500066
          00000000
          000cc000
          000cc000
          09000000
          09000dd0
          00000dd0
        ",
        // 18 confetti B
        "
          00770000
          00770000
          00000990
          00000990
          00000000
          55000000
          55000dd0
          00000dd0
        ",
        // 19 gold sparkle
        "
          00066000
          00066000
          06666660
          06666660
          00066000
          00066000
          00000000
          00000000
        ",
        // 20 flag-bearer bottom-left
        "
          0004bb10
          00555550
          00555555
          00055550
          00011110
          00044440
          00040040
          00110110
        ",
        // 21 flag-bearer bottom-right
        "
          000a0000
          000a0000
          555a0000
          000a0000
          000a0000
          00000000
          00000000
          00000000
        ",
        // 22 big robot bottom-left
        "
          00222222
          02233333
          02233366
          01133366
          00022222
          00022200
          00022200
          00122210
        ",
        // 23 big robot bottom-right
        "
          22222200
          33333220
          33333220
          33333110
          22222000
          00222000
          00222000
          00122210
        ",
    ];
    let bunting = [
        (TRI, '5'),
        (SWAL, '7'),
        (TRI, '6'),
        (SWAL, '8'),
        (TRI, '9'),
        (SWAL, 'c'),
        (TRI, 'd'),
        (SWAL, '6'),
    ];
    fixed
        .iter()
        .enumerate()
        .map(|(n, s)| {
            let raw = if (8..16).contains(&n) {
                let (shape, c) = bunting[n - 8];
                shape.replace('C', &c.to_string())
            } else {
                s.to_string()
            };
            raw.chars().filter(|c| !c.is_whitespace()).collect()
        })
        .collect()
}

fn parade_sheet() -> Vec<u8> {
    let cols = 16usize;
    let (w, h) = (cols * 8, 16usize);
    let mut buf = vec![0u8; w * h * 4];
    for (n, cell) in parade_cells().iter().enumerate() {
        assert_eq!(cell.len(), 64, "parade cell {n} is not 8x8");
        let (ox, oy) = ((n % cols) * 8, (n / cols) * 8);
        for y in 0..8 {
            for x in 0..8 {
                let idx = cell.as_bytes()[y * 8 + x] as char;
                let idx = idx.to_digit(16).unwrap() as usize;
                if idx == 0 {
                    continue;
                }
                let (r, g, b) = PARADE_PAL[idx];
                let i = ((oy + y) * w + ox + x) * 4;
                buf[i..i + 4].copy_from_slice(&[r, g, b, 255]);
            }
        }
    }
    buf
}

// ── the street (mirrors STREET_PAL / streetIndex) ────────────────────────────
const STREET_PAL: [(u8, u8, u8); 9] = [
    (0x00, 0x00, 0x00), // 0 transparent sky
    (0xf0, 0xf0, 0xf8), // 1 picket white
    (0x98, 0x98, 0xb8), // 2 picket shade / rails
    (0x68, 0x58, 0x78), // 3 cobble A
    (0x58, 0x50, 0x68), // 4 cobble B
    (0xa8, 0x98, 0xb0), // 5 kerb
    (0xf8, 0xd0, 0x60), // 6 sun
    (0xf8, 0xe8, 0xa8), // 7 sun core
    (0xd8, 0xc8, 0xe8), // 8 cloud (kept well off picket white)
];

const GROUND: i32 = 180;
const FENCE_X0: i32 = 92;
const FENCE_X1: i32 = 164;

fn street_index(x: i32, y: i32) -> usize {
    if y >= GROUND {
        if y < GROUND + 4 {
            return 5;
        }
        return if (x / 8 + (y - GROUND) / 8) % 2 == 0 {
            3
        } else {
            4
        };
    }
    if y == 4 {
        return 2; // the bunting rope — sprite pennants hang off it
    }
    if x >= FENCE_X0 && x < FENCE_X1 {
        let fx = (x - FENCE_X0) % 8;
        if (156..160).contains(&y) && (1..3).contains(&fx) {
            return 1;
        }
        if y >= 160 && fx < 4 {
            return if fx == 3 { 2 } else { 1 };
        }
        if (168..172).contains(&y) {
            return 2;
        }
    }
    let (sx, sy) = (x - 204, y - 44);
    if sx * sx + sy * sy < 100 {
        return 7;
    }
    if sx * sx + sy * sy < 256 {
        return 6;
    }
    // puffy flat-bottomed clouds: three discs clipped at a straight base line
    let cloud = |cx: i32, cy: i32| -> bool {
        if y > cy + 6 {
            return false;
        }
        let a = (x - cx + 13) * (x - cx + 13) + (y - cy) * (y - cy) < 81;
        let b = (x - cx) * (x - cx) + (y - cy + 7) * (y - cy + 7) < 144;
        let c = (x - cx - 13) * (x - cx - 13) + (y - cy) * (y - cy) < 64;
        a || b || c
    };
    if cloud(56, 60) || cloud(150, 84) || cloud(224, 110) {
        return 8;
    }
    0
}

fn street() -> Vec<u8> {
    let (w, h) = (256usize, 224usize);
    let mut buf = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let idx = street_index(x as i32, y as i32);
            if idx == 0 {
                continue;
            }
            let (r, g, b) = STREET_PAL[idx];
            let i = (y * w + x) * 4;
            buf[i..i + 4].copy_from_slice(&[r, g, b, 255]);
        }
    }
    buf
}

// ── engine + render ──────────────────────────────────────────────────────────
fn engine() -> LuaEngine {
    common::engine_with(
        &mut |e| {
            common::add_obj(e, "parade", parade_sheet(), 128, 16);
            common::add_bg(e, "street", street(), 256, 224, 4);
        },
        &[("main.lua", MAIN_SRC)],
    )
}

fn render() -> (Vec<u8>, LuaEngine) {
    let mut e = engine();
    let lt = e.frame(1.0, 60).unwrap();
    let fb = render_frame(&lt, e.memory());
    (fb, e)
}

/// A CGRAM colour as the compositor emits it (authored RGB through the
/// rgb15 grid and back).
fn c(r: u8, g: u8, b: u8) -> [u8; 4] {
    unpack_rgb15(rgb15(r, g, b))
}

// ── behaviour ────────────────────────────────────────────────────────────────

#[test]
fn sprite_parade_binds_sources_and_populates_oam() {
    let (fb, e) = render();
    assert!(
        !e.import_reports()
            .iter()
            .any(|r| matches!(r, ImportBudget::Mismatch { .. })),
        "a bound source mismatched its target slot"
    );
    let oam = &e.memory().oam;
    // The taught OAM surface, sprite by sprite: two larges, a flip, the gold
    // palette, and the prio-0/prio-3 pair the fence lesson hangs on.
    assert!(
        oam[0].on && oam[0].large,
        "flag-bearer should be on + large"
    );
    assert!(oam[1].on && oam[1].large, "big robot should be on + large");
    assert!(oam[4].on && oam[4].flip_x, "marcher 4 should be flipped");
    assert_eq!(oam[5].pal, 1, "gold marcher should use OBJ palette 1");
    assert_eq!(oam[7].prio, 0, "hidden marcher should be prio 0");
    assert_eq!(oam[8].prio, 3, "front marcher should be prio 3");
    assert_eq!(oam.iter().filter(|o| o.on).count(), 21);
    assert!(fb.chunks_exact(4).any(|px| px[3] == 255));
}

/// The sheet's whole numbering scheme: cell 0 dedups onto the reserved blank
/// tile and every used cell is unique (under flips), so sheet cell N = OBJ
/// tile N with no importer flips and one shared palette. The Lua's tile
/// numbers (and the 2x2 large-sprite blocks at 4/20 and 6/22) rely on this.
#[test]
fn sprite_parade_sheet_cells_map_one_to_one() {
    let opts = ConvertOptions {
        cell_size: Some(8),
        ..Default::default()
    };
    let (_, meta) = convert_source(SourceKind::Obj, &opts, &parade_sheet(), 128, 16).unwrap();
    let cells = meta.cells.expect("obj import reports per-cell attributes");
    assert_eq!(cells.len(), 32); // 16x2 grid
    for (n, cell) in cells.iter().enumerate().take(24) {
        let want = if n == 0 { 0 } else { n as u16 };
        assert_eq!(cell.tile, want, "cell {n} landed on the wrong tile");
        assert!(!cell.flip_x && !cell.flip_y, "cell {n} got a dedup flip");
        assert_eq!(cell.pal, 0, "cell {n} left the single shared palette");
    }
    for cell in &cells[24..] {
        assert_eq!(cell.tile, 0); // unused cells collapse onto the blank tile
    }
}

/// The priority lesson, pixel for pixel: at t=1 the prio-0 marcher stands in
/// the fence (x 120..128) and the prio-3 marcher over it (x 144..152).
/// Mode 1 puts OBJ prio 0 under the low-priority BG plane and prio 3 over it.
#[test]
fn sprite_parade_prio0_hides_behind_fence_and_prio3_walks_over_it() {
    let (fb, _) = render();
    let white = c(0xf0, 0xf0, 0xf8);
    // sanity: this column IS a picket where no sprite stands
    assert_eq!(common::px(&fb, 125, 157), &white, "picket sanity check");
    // prio 0: the picket wins the pixel...
    assert_eq!(
        common::px(&fb, 125, 175),
        &white,
        "prio-0 marcher should be occluded by the picket"
    );
    // ...but the marcher shows in the transparent gap next to it (pose A
    // outline pixel at sprite-local (2,3)).
    assert_eq!(
        common::px(&fb, 122, 175),
        &c(0x10, 0x18, 0x20),
        "prio-0 marcher should show through the picket gap"
    );
    // prio 3: the sprite wins over the same fence (pose B steel at (4,3)).
    assert_eq!(
        common::px(&fb, 148, 175),
        &c(0x40, 0x50, 0x68),
        "prio-3 marcher should draw over the picket"
    );
}

// ── golden ───────────────────────────────────────────────────────────────────

#[test]
fn sprite_parade_matches_golden_png() {
    assert!(Path::new(GOLDEN).exists(), "run the regen test first");
    let (actual, _) = render();
    let expected = common::decode_png(GOLDEN);
    assert_eq!(actual.len(), expected.len());
    assert!(
        actual == expected,
        "sprite-parade framebuffer differs from golden PNG"
    );
}

#[test]
#[ignore = "regenerates the committed sprite-parade golden PNG"]
fn regen_golden_sprite_parade() {
    let (fb, _) = render();
    common::write_png(GOLDEN, &fb);
}
