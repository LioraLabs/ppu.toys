//! Tutorial toy 8/10 :: transitions — the Rust mirror of
//! web/src/studio/demos/tutorials/transitions.ts (the Lua is verbatim; the
//! golden PNG proves the t=1.0 frame the studio ships: the scene mid-wipe).
mod common;

use ppu_core::{render_frame, HEIGHT, WIDTH};
use std::path::Path;

/// Byte-identical to MAIN_SRC in web/src/studio/demos/tutorials/transitions.ts.
const MAIN_SRC: &str = r#"-- ppu.toys tutorial 8/10 :: transitions — the scene-change toolkit built into the chip
-- Every SNES fade, flash, and level-exit is one of three registers plus a trick:
--   brightness    INIDISP bits 0-3 — the master fade, 0 (black) .. 15 (full)
--   mosaic        MOSAIC $2106 — block size 0..15; bg[n].mosaic is the same
--                 register's per-layer enable bit
--   force_blank   INIDISP bit 7 — the hard cut: the frame goes black NOW,
--                 whatever brightness says
--   the trick     write brightness inside hdma() and the fade lands per
--                 SCANLINE — a wipe down the screen no whole-frame fade can do
--
-- One scene, one 12-second loop, every transition in turn:
--   0-2    wipe-in      hdma brightness front sweeps down, revealing the scene
--   2-4    hold         the scene, untouched
--   4-6    fade         brightness 15 -> 0 -> 15
--   6-8    mosaic       block size 0 -> 15 -> 0: the picture chunks apart
--   8-10   mosaic+fade  both at once — the classic SNES level-exit
--   10-12  hold         ...cut by a force_blank blink at 11s; the loop's
--                       wipe-in reopens from that black
local CYCLE = 12

-- Setup stage: one dma copies the 8bpp scene into VRAM at compile — the
-- loading screen (parallax-skyline, lesson 2, tells the full story).
local vista = dma("vista", { char = 0x1000, map = 0x0000 })

function frame(t, f)
  apply_pokes()
  mode = 3                       -- 8bpp BG1: one full-colour scene to transition over
  bg[1].char_base = vista.char
  bg[1].map_base = vista.map
  bg[1].mosaic = true            -- enabling is free: size 0 below means "off"
  brightness = 15                -- the holds ARE these two defaults;
  mosaic = 0                     -- each phase only overrides what it needs

  local tc = t % CYCLE           -- t grows forever: schedule off the remainder
  local phase = floor(tc / 2)    -- six 2-second phases
  local u = tc - phase * 2       -- 0..2 inside the current phase

  if phase == 0 then
    -- WIPE-IN. brightness written inside hdma() lands on ONE scanline, so a
    -- moving front with a short ramp reads as a curtain lifting down the frame.
    local edge = u * 144         -- the front's scanline, sweeping past 223
    hdma(0, 223, function(y)
      brightness = min(15, max(0, floor((edge - y) / 2)))  -- full 30px above the front, black below
    end)
  elseif phase == 2 then
    -- FADE. One register, out and back: 15 -> 0 -> 15 across the two seconds.
    brightness = floor(15 * abs(u - 1))
  elseif phase == 3 then
    -- MOSAIC. The same triangle on the block size; bg[1].mosaic opted BG1 in.
    mosaic = floor(15 * (1 - abs(u - 1)))
  elseif phase == 4 then
    -- BOTH. Chunk apart while dimming — the level-exit half the SNES library used.
    local k = 1 - abs(u - 1)
    mosaic = floor(15 * k)
    brightness = 15 - floor(11 * k)   -- dim to 4, not 0: keep the chunks readable
  elseif phase == 5 and u >= 1 and u < 1.15 then
    -- THE CUT. force_blank blanks the frame no matter what brightness says.
    force_blank = true
  end
  -- (phases 1 and 5 otherwise fall through: the hold is the defaults above)
end
-- Try: retime the schedule (CYCLE and the /2), steepen the wipe (the /2 in the
-- hdma), let the level-exit fade all the way out, or blank the whole last phase.
"#;

const GOLDEN: &str = "tests/fixtures/golden_tutorial_transitions.png";
const HORIZON: usize = 140;

/// |((x % p) - p/2)| — integer triangle wave (mirrors tri() in transitions.ts).
fn tri(x: usize, p: usize) -> i32 {
    ((x % p) as i32 - (p / 2) as i32).abs()
}

/// Mirrors scene() in transitions.ts byte-for-byte: dusk vista with sub-8px
/// detail for the mosaic phases to flatten, integer math only.
fn vista() -> Vec<u8> {
    let mut data = vec![0u8; WIDTH * HEIGHT * 4];
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let i = (y * WIDTH + x) * 4;
            let (r, g, b): (i32, i32, i32) = if y >= HORIZON {
                // water: darkens with depth; bright 2px flecks drift per row
                let db = ((y - HORIZON) / 4) as i32;
                let fleck = (x + y * 5) % 8 < 2;
                (
                    144 - db * 4 + if fleck { 64 } else { 0 },
                    72 - db * 2 + if fleck { 56 } else { 0 },
                    56 + db * 2 + if fleck { 32 } else { 0 },
                )
            } else {
                let r1 = (70 + tri(x + 16, 64)) as usize; // far ridge top: 70..102
                let r2 = (104 + tri(x + 96, 128) / 2) as usize; // near ridge top: 104..136
                if y >= r2 {
                    // near ridge: dark with a 1px checker shimmer
                    let c = if (x + y) % 2 == 0 { 1 } else { 0 };
                    (48 + c * 10, 32 + c * 8, 72 + c * 12)
                } else if y >= r1 {
                    (94, 62, 118) // far ridge, flat violet
                } else if (176..200).contains(&x) && (40..64).contains(&y) {
                    (255, 224, 152) // the sun, an 8px-aligned square
                } else {
                    // dusk sky gradient, stepped every 4 rows
                    let band = (y / 4) as i32;
                    (32 + band * 4, 28 + band * 2, 96 - band * 2)
                }
            };
            data[i] = r as u8;
            data[i + 1] = g as u8;
            data[i + 2] = b as u8;
            data[i + 3] = 255;
        }
    }
    data
}

/// Render the toy at an arbitrary schedule point (the golden uses t=1.0/f=60).
fn render_at(t: f64, f: u32) -> Vec<u8> {
    let mut e = common::engine_with(
        &mut |e| common::add_bg(e, "vista", vista(), WIDTH as u32, HEIGHT as u32, 8),
        &[("main.lua", MAIN_SRC)],
    );
    let lt = e.frame(t, f).unwrap();
    render_frame(&lt, e.memory())
}

fn lum(p: &[u8]) -> u32 {
    p[..3].iter().map(|&c| c as u32).sum()
}

#[test]
fn t1_wipe_frame_shows_the_scene_above_a_darkening_front() {
    // t=1.0 -> phase 0, u=1.0, edge=144: full brightness above ~y=114, a ramp
    // down to the front, black below — the golden frame, mid-transition.
    let fb = render_at(1.0, 60);
    let sky = common::px(&fb, 128, 20);
    assert_eq!(sky[3], 255);
    assert_ne!(&sky[..3], &[0, 0, 0], "sky above the front was black");
    let sun = common::px(&fb, 188, 50);
    assert!(
        sun[0] > 200,
        "sun should be bright at full brightness: {sun:?}"
    );
    let below = common::px(&fb, 128, 200);
    assert_eq!(&below[..3], &[0, 0, 0], "below the front must be black");
    // inside the ramp: dimmer than the same pixel at the hold, but not black
    let band = common::px(&fb, 128, 132);
    let hold = render_at(3.0, 60);
    let held = common::px(&hold, 128, 132);
    assert_ne!(&band[..3], &[0, 0, 0], "ramp pixel already black");
    assert!(
        lum(band) < lum(held),
        "per-scanline brightness should dim the ramp: wipe {band:?} vs hold {held:?}"
    );
}

#[test]
fn fade_phase_is_dimmer_than_the_hold() {
    // t=4.5 -> phase 2, u=0.5 -> brightness = floor(15 * 0.5) = 7.
    let hold = render_at(3.0, 60);
    let fade = render_at(4.5, 60);
    let (h, f) = (common::px(&hold, 128, 80), common::px(&fade, 128, 80));
    assert!(
        lum(f) < lum(h),
        "mid-fade should be dimmer: hold {h:?} vs fade {f:?}"
    );
    assert_ne!(&f[..3], &[0, 0, 0], "mid-fade should not be black yet");
}

#[test]
fn mosaic_phase_flattens_the_water_flecks() {
    // Row 141 has a fleck at x=0 but not x=2, so the pixels differ at the hold;
    // t=7.0 -> mosaic = 15 -> 16px blocks replicate one sample across both.
    let hold = render_at(3.0, 60);
    assert_ne!(
        common::px(&hold, 0, 141),
        common::px(&hold, 2, 141),
        "the fine detail the mosaic test relies on is missing"
    );
    let mosaic = render_at(7.0, 60);
    assert_eq!(
        common::px(&mosaic, 0, 141),
        common::px(&mosaic, 2, 141),
        "mosaic size 15 should flatten a 16px block to one sample"
    );
}

#[test]
fn combo_phase_chunks_and_dims_at_once() {
    // t=9.0 -> phase 4, k=1: mosaic 15 AND brightness 4 — the level-exit.
    let hold = render_at(3.0, 60);
    let combo = render_at(9.0, 60);
    assert_eq!(
        common::px(&combo, 0, 141),
        common::px(&combo, 2, 141),
        "combo phase should still mosaic"
    );
    assert!(
        lum(common::px(&combo, 128, 80)) < lum(common::px(&hold, 128, 80)),
        "combo phase should also dim"
    );
}

#[test]
fn force_blank_blink_cuts_the_whole_frame_to_black() {
    // t=11.05 -> phase 5, u in [1, 1.15): force_blank = true.
    let fb = render_at(11.05, 60);
    assert!(
        fb.chunks_exact(4).all(|p| p[..3] == [0, 0, 0]),
        "force_blank frame was not fully black"
    );
}

#[test]
fn transitions_matches_golden_png() {
    assert!(Path::new(GOLDEN).exists());
    let actual = render_at(1.0, 60);
    let expected = common::decode_png(GOLDEN);
    assert_eq!(actual.len(), expected.len());
    assert!(
        actual == expected,
        "transitions framebuffer differs from golden PNG"
    );
}

#[test]
#[ignore = "regenerates the committed transitions golden PNG"]
fn regen_golden_transitions() {
    let fb = render_at(1.0, 60);
    common::write_png(GOLDEN, &fb);
}
