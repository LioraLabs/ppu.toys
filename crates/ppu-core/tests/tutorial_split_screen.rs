//! Tutorial toy 7/10 :: split-screen — the Rust mirror of
//! web/src/studio/demos/tutorials/splitScreen.ts (the Lua is verbatim, the
//! city()/floor_tex() pixels are byte-identical; the golden PNG proves the
//! frame the studio ships). One frame, two hardware modes: the frame-wide
//! default is mode 1 (city), and `mode = 7` inside the hdma hook flips
//! scanlines 112..223 to the affine floor. Both payloads are placed by
//! top-level `dma()` — the city at explicit addresses, the floor into the
//! interleaved m7 region — the PPU-106 coexistence this toy now teaches.
mod common;

use ppu_core::render_frame;
use std::path::Path;

/// Byte-identical to MAIN_SRC in web/src/studio/demos/tutorials/splitScreen.ts.
const MAIN_SRC: &str = r#"-- ppu.toys tutorial 7/10 :: split-screen -- two hardware modes in ONE frame
--
-- A background MODE is the PPU's whole personality: how many layers you get
-- and how many colours each one has. Mode 1 is the workhorse tile mode; mode 7
-- is the single affine plane every racing game steered. The chip reads the
-- mode from one register -- BGMODE ($2105) -- and it reads it EVERY scanline.
-- Real SNES games split the frame by pointing an HDMA channel at BGMODE: tile
-- scenery up top, a perspective floor below. Setting that up on hardware took
-- a DMA channel and a table in WRAM. Here it is one line: assign 'mode' INSIDE
-- an hdma hook and only those scanlines flip. A per-scanline mode switch is a
-- trick only this DSL does.
--
-- Setup stage: BOTH worlds go into VRAM up front, each by its payload's own
-- rules (parallax-skyline, lesson 2, tells the full dma story). The city is
-- tile data at addresses we pick; a mode 7 texture always lands interleaved
-- at 0x0000-0x3FFF (that region is hard-wired into the chip), which is
-- exactly why the city's tiles park above it at 0x4000. dma places by what
-- the data IS, not by what mode the frame runs in -- that is the whole
-- reason one frame can hold both worlds.
local city = dma("city", { char = 0x4000, map = 0x7000 })
dma("floor")   -- m7: chars+map at 0x0000, its 3-colour palette at CGRAM 1..
-- One palette serves both bands: CGRAM has no notion of the split. The floor
-- is deliberately painted with three colours the city already owns, so its
-- palette lands on entries 1..3 holding the exact same values -- shared
-- paint instead of a fight.
function frame(t, f)
  apply_pokes()
  mode = 1; brightness = 15    -- frame-wide default: every line starts in mode 1

  -- Top band: point BG1 at the city's VRAM home from the setup stage.
  bg[1].char_base = city.char  -- clear of the mode 7 words (0x0000-0x3FFF)
  bg[1].map_base = city.map
  screen.main.bg1 = true
  screen.main.bg2 = false; screen.main.bg3 = false  -- power-on defaults ALL layers
  screen.main.bg4 = false; screen.main.obj = false  -- on; they'd show garbage here

  cgram[0] = rgb(16, 8, 40)    -- backdrop, should anything miss

  -- The split. The hook runs once per scanline y; every register assignment
  -- inside lands on that line ONLY.
  local split = 112
  hdma(split, 223, function(y)
    mode = 7                          -- THE line: mode 7 for scanlines 112..223
    local d = 64 / (y - (split - 1))  -- perspective divide: far rows step the
    m7.a, m7.d = d, d                 -- texture faster, near rows slower (mode7-road)
    m7.cx, m7.cy = 128, 0             -- vanishing point sits on the split
    bg[1].scroll.y = t * 80 * d       -- drive forward; scaling by d keeps depth honest
  end)
end
-- Try: move the split with 112 + floor(sin(t) * 16) (both uses of 'split' follow);
-- steer with m7.cx = 128 + sin(t) * 40; or swap the floor for mode7-road's
-- ground: drag any image into assets as an m7 texture and change the name in
-- dma("floor").
"#;

const GOLDEN: &str = "tests/fixtures/golden_tutorial_split_screen.png";
const SPLIT: usize = 112;

/// Byte-identical mirror of city() in splitScreen.ts: dusk skyline in rows
/// 0..112, transparent below (those rows never show in mode 1). 9 colours,
/// all channels multiples of 8, integer math only.
fn city() -> Vec<u8> {
    const W: i32 = 256;
    let mut data = vec![0u8; 256 * 224 * 4];
    const SKY_BANDS: [(i32, u8, u8, u8); 5] = [
        (20, 24, 16, 64),
        (44, 56, 24, 88),
        (68, 104, 40, 96),
        (88, 168, 64, 88),
        (SPLIT as i32, 224, 104, 72),
    ];
    for y in 0..SPLIT as i32 {
        for x in 0..W {
            let (mut r, mut g, mut b) = (0u8, 0u8, 0u8);
            for &(limit, br, bg, bb) in &SKY_BANDS {
                if y < limit {
                    (r, g, b) = (br, bg, bb);
                    break;
                }
            }
            // low sun, partly hidden behind the far skyline
            let (dx, dy) = (x - 200, y - 56);
            if dx * dx + dy * dy < 196 {
                (r, g, b) = (248, 224, 152);
            }
            // far skyline (lighter silhouette), 3px gaps so the dusk glow leaks through
            let far_top = 58 + ((x / 16 * 13) % 5) * 6;
            if x % 16 < 13 && y >= far_top {
                (r, g, b) = (72, 48, 104);
            }
            // near buildings (dark), 2px gaps between blocks
            let gap = x % 24 >= 22;
            let near_top = 68 + ((x / 24 * 37) % 33);
            if !gap && y >= near_top {
                (r, g, b) = (16, 16, 32);
                // lit windows: 2x2 cells on a 6x8 grid, about half of them on
                let wx = x % 24;
                if y >= near_top + 3
                    && y < 108
                    && (2..=3).contains(&(wx % 6))
                    && (2..=3).contains(&(y % 8))
                    && (x / 6 * 5 + y / 8 * 3) % 4 < 2
                {
                    (r, g, b) = (248, 208, 88);
                }
            }
            let i = ((y * W + x) * 4) as usize;
            data[i] = r;
            data[i + 1] = g;
            data[i + 2] = b;
            data[i + 3] = 255;
        }
    }
    data
}

/// Byte-identical mirror of floorTex() in splitScreen.ts: a 1024x1024 mode 7
/// texture — a sunset neon grid (32px period, 2px seams) over 8px-checkered
/// asphalt. The three colours are EXACTLY the city's three lowest BGR555
/// palette entries, and they sort into the same order under the m7 importer's
/// [r,g,b] palette sort, so both dma placements write the same values into
/// the shared CGRAM 1..3 (`floor_palette_shares_the_citys_entries` pins this).
fn floor_tex() -> Vec<u8> {
    let mut data = vec![0u8; 1024 * 1024 * 4];
    for y in 0..1024usize {
        for x in 0..1024usize {
            let (r, g, b) = if x % 32 < 2 || y % 32 < 2 {
                (224u8, 104u8, 72u8) // sunset grid seams
            } else if ((x / 8) + (y / 8)) % 2 == 1 {
                (24, 16, 64) // asphalt B (indigo)
            } else {
                (16, 16, 32) // asphalt A (navy)
            };
            let i = (y * 1024 + x) * 4;
            data[i..i + 4].copy_from_slice(&[r, g, b, 255]);
        }
    }
    data
}

fn engine() -> ppu_core::LuaEngine {
    common::engine_with(
        &mut |e| {
            common::add_bg(e, "city", city(), 256, 224, 4);
            common::add_m7(e, "floor", floor_tex(), 1024, 1024);
        },
        &[("main.lua", MAIN_SRC)],
    )
}

/// The t=1.0 / f=60 frame the golden pins.
fn render() -> Vec<u8> {
    let mut e = engine();
    let lt = e.frame(1.0, 60).unwrap();
    render_frame(&lt, e.memory())
}

#[test]
fn the_split_lands_where_the_hdma_says() {
    let mut e = engine();
    let lt = e.frame(1.0, 60).unwrap();
    assert_eq!(lt.rows[0].mode, 1, "top band should be mode 1");
    assert_eq!(
        lt.rows[SPLIT - 1].mode,
        1,
        "last city line should be mode 1"
    );
    assert_eq!(lt.rows[SPLIT].mode, 7, "first floor line should be mode 7");
    assert_eq!(lt.rows[223].mode, 7, "bottom line should be mode 7");
}

#[test]
fn top_band_shows_the_mode1_city() {
    let fb = render();
    // dusk sky up top: blue dominates
    let sky = common::px(&fb, 128, 8);
    assert_eq!(sky[3], 255);
    assert!(
        sky[2] > sky[0] && sky[2] > sky[1],
        "sky at (128,8) should be indigo: {sky:?}"
    );
    // a near building low in the band: dark navy silhouette
    let bld = common::px(&fb, 8, 104);
    assert!(
        bld[..3].iter().map(|&c| c as u32).sum::<u32>() < 140,
        "building at (8,104) should be dark: {bld:?}"
    );
    assert_ne!(sky, bld, "the city art should vary within the band");
}

#[test]
fn bottom_band_shows_the_mode7_floor() {
    let fb = render();
    // Every pixel on a floor row is one of the dma-placed texture's three
    // colours (through the rgb15 grid and back) — proof the m7 payload landed
    // and its shared palette entries survived the city's placement.
    let floor_colors: [[u8; 4]; 3] = [
        ppu_core::unpack_rgb15(ppu_core::rgb15(16, 16, 32)),
        ppu_core::unpack_rgb15(ppu_core::rgb15(24, 16, 64)),
        ppu_core::unpack_rgb15(ppu_core::rgb15(224, 104, 72)),
    ];
    let mut distinct = std::collections::HashSet::new();
    for x in (8..248).step_by(8) {
        let p = common::px(&fb, x, 200);
        assert!(
            floor_colors.iter().any(|c| c == p),
            "floor pixel at ({x},200) is not a floor-texture colour: {p:?}"
        );
        distinct.insert([p[0], p[1], p[2]]);
    }
    assert!(
        distinct.len() >= 2,
        "the checker/grid texture should show at least two colours on row 200"
    );
}

/// The palette-sharing invariant the Lua header teaches: the floor's three
/// colours ARE the city's three lowest BGR555 entries, in the same order, so
/// after both placements CGRAM 1..3 hold exactly those values (and the city's
/// remaining entries are untouched by the floor).
#[test]
fn floor_palette_shares_the_citys_entries() {
    let mut e = engine();
    e.frame(1.0, 60).unwrap();
    let cg = &e.memory().cgram;
    assert_eq!(cg[1], ppu_core::rgb15(16, 16, 32), "entry 1 (navy)");
    assert_eq!(cg[2], ppu_core::rgb15(24, 16, 64), "entry 2 (indigo)");
    assert_eq!(cg[3], ppu_core::rgb15(224, 104, 72), "entry 3 (sunset)");
}

#[test]
fn split_screen_matches_golden_png() {
    assert!(Path::new(GOLDEN).exists());
    let actual = render();
    let expected = common::decode_png(GOLDEN);
    assert_eq!(actual.len(), expected.len());
    assert!(
        actual == expected,
        "split-screen framebuffer differs from golden PNG"
    );
}

#[test]
#[ignore = "regenerates the committed split-screen golden PNG"]
fn regen_golden_split_screen() {
    let fb = render();
    common::write_png(GOLDEN, &fb);
}
