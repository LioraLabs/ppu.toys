//! parallax-skyline (L1 tutorial toy 2 of 10) golden + behavior tests through
//! the real Lua/importer/render pipeline.
//!
//! Lua sources and asset generators mirror
//! web/src/studio/demos/tutorials/parallaxSkyline.ts byte-for-byte — these MUST
//! agree or the golden proves nothing about the shipped toy. Edit both.
mod common;

use ppu_core::{render_frame, ImportBudget, LuaEngine, HEIGHT, WIDTH};

const GOLDEN: &str = "tests/fixtures/golden_tutorial_parallax_skyline.png";

const MAIN_SRC: &str = r#"-- ppu.toys :: parallax-skyline — tutorial 2 of 10 (after first-light; mode7-road is next)
--
-- A night city in THREE depths from TWO tile layers:
--   bg[2]  far   — stars, moon, distant towers     (slow scroll)
--   bg[1]  near  — mid buildings, above y=168      (faster scroll)
--   bg[1]  near  — foreground strip, below y=168   (fastest — via hdma)
-- The third depth is free: an hdma hook rewrites bg[1]'s scroll per scanline,
-- so ONE layer scrolls at two speeds — the classic SNES parallax-strip trick.
--
-- THE SETUP STAGE. Code outside frame() runs ONCE, when the toy compiles —
-- that is your loading screen. It is where real games ran their DMA: copying
-- chars, tilemaps and palettes from the cartridge into VRAM before the first
-- frame ever drew. dma(name, opts) is that same hardware op. VRAM is one
-- shared 64KB pool with no owner but you, so every image states its own
-- addresses — two images must not overlap:
local near = dma("skyline_near", { char = 0x1000, map = 0x0000 })
local far  = dma("skyline_far",  { char = 0x4000, map = 0x0800 })
-- From here on VRAM is the only truth. A layer never sees an image: inside
-- frame(), bg[n].char_base/map_base simply point INTO VRAM — the same two
-- registers (BGnNBA/BGnSC) the chip itself reads. dma returns the resolved
-- addresses, so the registers wire from the placement instead of repeating it.
--
-- MULTI-FILE: this toy has two tabs. Chunks run in tab order into ONE shared
-- global scope (PICO-8 style), so FAR_SPEED and band_speed() from skyline.lua
-- are plain globals here. main.lua is a convention, not magic.
function frame(t, f)
  apply_pokes()
  mode = 1; brightness = 15

  -- DESIGNATE THE LAYERS. Both screens power on EMPTY: TM (screen.main) says
  -- which layers reach the main screen — the picture — and TS (screen.sub)
  -- which reach the sub screen, the one colour math blends in (stage-lights,
  -- lesson 6). A layer you never designate never draws, so a layer you never
  -- set up can never rasterize whatever VRAM happens to hold.
  screen.main.bg1 = true; screen.main.bg2 = true

  -- Point each layer at the VRAM the setup stage filled. In mode 1, bg1
  -- draws over bg2, so the near image went to bg1 and is transparent
  -- (alpha 0) wherever the sky and far towers must show through.
  bg[1].char_base = near.char; bg[1].map_base = near.map
  bg[2].char_base = far.char;  bg[2].map_base = far.map

  -- Depth = scroll speed. The far layer creeps...
  bg[2].scroll.x = t * FAR_SPEED

  -- ...and the near layer's speed depends on the SCANLINE. Register writes
  -- inside an hdma hook are per-scanline, so this gives bg1 a different scroll
  -- on every row — one layer, several depth bands.
  hdma(0, 223, function(y)
    bg[1].scroll.x = t * band_speed(y)
  end)
end
-- Try: add { y = 208, speed = 140 } to BANDS (the street glow pulls ahead),
--      slow FAR_SPEED to 4 in skyline.lua,
--      or drift the sky: bg[2].scroll.y = sin(t) * 4
"#;

const SKYLINE_SRC: &str = r#"-- parallax-skyline :: skyline.lua — shared constants + the depth-band table.
-- This chunk runs BEFORE frame() is ever called; everything here is a global,
-- visible from every other tab (one shared scope — see main.lua's header).

FAR_SPEED = 12          -- px/sec for the far layer (bg2)

-- The near layer's depth bands: from row .y down to the next entry, scroll at
-- .speed px/sec. The split at 168 matches the art — the image's foreground
-- strip starts on exactly that row, so the seam between bands is invisible.
BANDS = {
  { y = 0,   speed = 40 },   -- mid buildings
  { y = 168, speed = 90 },   -- foreground rooftops + street
}

-- Speed for scanline y: the last band starting at or above y.
function band_speed(y)
  local s = BANDS[1].speed
  for i = 2, #BANDS do
    if y >= BANDS[i].y then s = BANDS[i].speed end
  end
  return s
end
"#;

/// The single-file concat of the toy's USER chunks (main + skyline), in web
/// tab order, for the multi-file parity test. pokes.lua is not part of this
/// concat — `common::engine_with` prepends it separately.
fn skyline_concat() -> String {
    format!("{MAIN_SRC}\n{SKYLINE_SRC}")
}

// ── asset generators (mirror parallaxSkyline.ts) ────────────────────────────
// One shared 14-colour master palette, both images using the whole set: outside
// mode 0 every BG source lands its palettes at CGRAM 0 and the second import
// lands on top, so identical colour SETS fitting ONE 4bpp sub-palette (14 <= 15)
// make the overwrite a no-op. Channels are multiples of 8 (the rgb15 grid).
const SKYLINE_PAL: [(u8, u8, u8); 15] = [
    (0, 0, 0),       //  0 = transparent, never placed
    (8, 8, 24),      //  1 sky, zenith
    (24, 16, 56),    //  2 sky, mid
    (48, 32, 80),    //  3 horizon glow / dark window glass
    (232, 232, 208), //  4 moon / rooftop beacon
    (192, 200, 224), //  5 star / penthouse glint
    (32, 24, 56),    //  6 far tower / rooftop tank
    (96, 80, 136),   //  7 far lit window
    (16, 8, 24),     //  8 foreground silhouette / shaded wall
    (40, 40, 64),    //  9 mid building body
    (248, 200, 88),  // 10 warm lit window
    (144, 96, 48),   // 11 dim lit window
    (56, 48, 80),    // 12 roof edge
    (88, 56, 96),    // 13 street glow
    (128, 168, 200), // 14 cool (office) window
];

const SPLIT: usize = 168;

/// Far layer palette index at (x, y) — mirrors `farIndex` in parallaxSkyline.ts.
fn far_index(x: usize, y: usize) -> u32 {
    if y >= 172 {
        return 13; // city-base glow the towers sink into
    }
    let (mx, my) = (x as i32 - 200, y as i32 - 40);
    if mx * mx + my * my < 169 {
        // the moon, with a shaded crater
        let (cx, cy) = (x as i32 - 196, y as i32 - 37);
        return if cx * cx + cy * cy < 9 { 5 } else { 4 };
    }
    if y < 108 && (x * x * 3 + x * 7 + y * y * 5 + y * 3) % 449 == 0 {
        return 5; // stars
    }
    // nearer rank of far towers (visible through the near layer's gaps)
    let b2 = x / 32;
    let h2 = 144 + ((b2 * 23) % 4) * 7;
    if y >= h2 {
        if y < h2 + 2 {
            return 12;
        }
        if x % 32 >= 30 {
            return 8;
        }
        if (3..5).contains(&(x % 8)) && (3..5).contains(&(y % 8)) {
            match ((x / 8) * 31 + (y / 8) * 17) % 7 {
                0 => return 10,
                1 => return 11,
                2 => return 14,
                _ => {}
            }
        }
        return 9;
    }
    // farthest rank: pure silhouettes with a few pale windows
    let b1 = x / 16;
    let h1 = 112 + ((b1 * 37) % 5) * 7;
    if y >= h1 {
        if (6..8).contains(&(x % 16)) && (2..4).contains(&(y % 8)) {
            match ((x / 16) * 13 + (y / 8) * 7) % 5 {
                0 => return 7,
                1 => return 14,
                _ => {}
            }
        }
        return 6;
    }
    // sky bands with a checker dither at each seam
    if y < 44 {
        return 1;
    }
    if y < 52 {
        return if (x + y) % 2 == 0 { 1 } else { 2 };
    }
    if y < 90 {
        return 2;
    }
    if y < 100 {
        return if (x + y) % 2 == 0 { 2 } else { 3 };
    }
    3
}

/// Near layer palette index at (x, y) — mirrors `nearIndex` in
/// parallaxSkyline.ts. 0 = transparent (the far layer shows).
fn near_index(x: usize, y: usize) -> u32 {
    if y >= SPLIT {
        // foreground strip: rooftop profile, sparse windows, street glow
        let c = x / 16;
        let hf = SPLIT + ((c * 13) % 3) * 4;
        if y < hf {
            return 0;
        }
        if y >= 216 {
            return 13;
        }
        if y >= 208 {
            return 3;
        }
        if y < hf + 1 {
            return 9; // roof lip catching the street light
        }
        if (6..8).contains(&(x % 16)) && (8..10).contains(&(y % 16)) {
            match (c * 7 + (y / 16) * 5) % 6 {
                0 => return 10,
                1 => return 11,
                2 => return 1,
                _ => {}
            }
        }
        return 8;
    }
    // mid buildings: a stepped roofline with gap slits the far layer shows through
    let b = x / 32;
    let hm = 92 + ((b * 29) % 5) * 11;
    if x % 32 >= 26 {
        return 0; // gap between buildings
    }
    if y < hm {
        // rooftop furniture above the roofline
        if (b * 29) % 5 == 0 {
            if y >= hm - 8 && y < hm - 6 && (15..17).contains(&(x % 32)) {
                return 4; // beacon
            }
            if y >= hm - 6 && (14..18).contains(&(x % 32)) {
                return 6; // its mast
            }
        }
        if (b * 29) % 5 == 2 && y >= hm - 6 && (10..20).contains(&(x % 32)) {
            return 6; // water tank
        }
        return 0;
    }
    if y < hm + 2 {
        return 12; // roof edge
    }
    if x % 32 >= 24 {
        return 8; // shaded wall
    }
    if y < hm + 4 && (20..22).contains(&((x + b * 11) % 32)) {
        return 5; // penthouse glint
    }
    if (3..6).contains(&(x % 8)) && (2..5).contains(&(y % 8)) {
        // the window grid: mostly dark glass, a scatter of lit ones
        match ((x / 8) * 31 + (y / 8) * 17) % 9 {
            0 | 1 => return 10,
            2 => return 14,
            3 => return 11,
            4 => return 7,
            5 | 6 => return 2,
            _ => return 1,
        }
    }
    9
}

fn paint(index_at: fn(usize, usize) -> u32) -> Vec<u8> {
    let mut data = vec![0u8; WIDTH * HEIGHT * 4];
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let idx = index_at(x, y);
            if idx == 0 {
                continue; // alpha 0, the layer behind shows
            }
            let (r, g, b) = SKYLINE_PAL[idx as usize];
            let i = (y * WIDTH + x) * 4;
            data[i..i + 4].copy_from_slice(&[r, g, b, 255]);
        }
    }
    data
}

fn add_assets(e: &mut LuaEngine) {
    common::add_bg(
        e,
        "skyline_far",
        paint(far_index),
        WIDTH as u32,
        HEIGHT as u32,
        4,
    );
    common::add_bg(
        e,
        "skyline_near",
        paint(near_index),
        WIDTH as u32,
        HEIGHT as u32,
        4,
    );
}

fn render_files(files: &[(&str, &str)]) -> (Vec<u8>, ppu_core::LineTable, LuaEngine) {
    let mut e = common::engine_with(&mut add_assets, files);
    let lt = e.frame(1.0, 60).unwrap();
    let fb = render_frame(&lt, e.memory());
    (fb, lt, e)
}

fn render_toy() -> (Vec<u8>, ppu_core::LineTable, LuaEngine) {
    render_files(&[("main.lua", MAIN_SRC), ("skyline.lua", SKYLINE_SRC)])
}

#[test]
fn both_bg_imports_bind_clean() {
    let (fb, _, e) = render_toy();
    assert!(
        !e.import_reports()
            .iter()
            .any(|r| matches!(r, ImportBudget::Mismatch { .. })),
        "a bound source mismatched its target slot"
    );
    assert!(fb
        .chunks_exact(4)
        .any(|p| p[3] == 255 && p[..3] != [0, 0, 0]));
}

#[test]
fn far_layer_shows_the_sky_up_top() {
    let (fb, _, _) = render_toy();
    // (20, 20) is sky through the near layer's transparent rows; the moon disc
    // sits near (200, 40) but scrolls with bg2 — probe the un-scrolled row for
    // a bright pixel instead of a fixed point.
    let sky = common::px(&fb, 20, 20);
    assert_eq!(sky[3], 255);
    assert_ne!(&sky[..3], &[0, 0, 0], "sky pixel was backdrop black");
    let moon_row_bright = (0..WIDTH).any(|x| common::px(&fb, x, 40)[0] > 180);
    assert!(moon_row_bright, "no moon-bright pixel on its scanline");
}

#[test]
fn near_layer_covers_the_lower_frame() {
    let (fb, _, _) = render_toy();
    let building = common::px(&fb, 100, 150);
    assert_eq!(building[3], 255);
    assert_ne!(&building[..3], &[0, 0, 0], "building pixel was backdrop");
    assert_ne!(
        building,
        common::px(&fb, 100, 20),
        "lower frame matches the sky — near layer missing"
    );
}

#[test]
fn hdma_bands_give_bg1_two_scroll_speeds_over_a_slow_bg2() {
    let (_, lt, _) = render_toy();
    // t = 1.0: upper band 40, lower band 90, far layer 12 on every row.
    assert_eq!(lt.rows[50].bg[0].scroll_x, 40);
    assert_eq!(lt.rows[200].bg[0].scroll_x, 90);
    assert_eq!(lt.rows[50].bg[1].scroll_x, 12);
    assert_eq!(lt.rows[200].bg[1].scroll_x, 12);
}

#[test]
fn multi_file_tab_order_concat_is_equivalent() {
    // The PICO-8 shared-scope contract the tutorial teaches: the two tabs
    // concatenated in tab order render the exact same frame.
    let (files_fb, _, _) = render_toy();
    let concat = skyline_concat();
    let (concat_fb, _, _) = render_files(&[("source", &concat)]);
    assert_eq!(files_fb, concat_fb);
}

#[test]
fn parallax_skyline_matches_golden_png() {
    assert!(std::path::Path::new(GOLDEN).exists());
    let (actual, _, _) = render_toy();
    let expected = common::decode_png(GOLDEN);
    assert_eq!(actual.len(), expected.len());
    assert!(
        actual == expected,
        "parallax-skyline framebuffer differs from golden PNG"
    );
}

#[test]
#[ignore = "regenerates the committed parallax-skyline golden PNG"]
fn regen_golden_parallax_skyline() {
    let (fb, _, _) = render_toy();
    common::write_png(GOLDEN, &fb);
}
