//! Tutorial toy 3/10 :: mode7-road — the Rust mirror of
//! web/src/studio/demos/tutorials/mode7Road.ts (the Lua is verbatim, road()
//! generates the same pixels; the golden PNG proves the frame the studio
//! ships).
mod common;

use ppu_core::render_frame;
use std::path::Path;

/// Byte-identical to MAIN_SRC in web/src/studio/demos/tutorials/mode7Road.ts.
const MAIN_SRC: &str = r#"-- ppu.toys tutorial 3/10 :: mode7-road — the affine matrix, the perspective
-- divide, and a ground plane you can drop your own photo onto.
--
-- Mode 7 is the SNES's one big "3D" trick, and it is nothing but sampling: BG1
-- becomes a single 1024x1024 image read through an affine transform. Four
-- matrix registers do all the work:
--   m7.a m7.d   texels stepped per screen pixel (the scale)
--   m7.b m7.c   the cross terms (rotation / shear — 0 here, so no turn)
--   m7.cx m7.cy the pivot the transform scales around
-- Held constant, that only zooms and spins a flat picture. The "3D" floor is a
-- per-scanline lie: an hdma hook rewrites the scale on EVERY line below the
-- horizon, dividing by how far down the screen the line sits. Far lines take
-- big steps across the image (so it draws small), near lines tiny ones. One
-- divide per line, and a flat image lies down into a road.
--
-- THE PART TO STEAL: "road" is only an image source. Drag ANY png onto the
-- assets panel and point bg[1].source at its name — your photo becomes the
-- ground plane. That drag-drop is the whole reason this site exists.
function frame(t, f)
  apply_pokes()
  mode = 7; brightness = 15        -- mode 7: BG1 is now the affine layer
  bg[1].source = "road"            -- the ground image — swap in your own here

  local HORIZON = 96               -- screen row where ground meets sky
  local SCALE   = 128              -- eye height, in effect: bigger = higher up
  local SPEED   = 160              -- driving speed, ground texels per second

  -- Sky. Above the horizon nothing should draw, so the backdrop (cgram[0])
  -- shows through: park the plane out of reach up there — wrap mode 2 makes
  -- off-plane samples transparent — then haze the backdrop toward the horizon
  -- with the fixed-colour gradient trick from first-light (tutorial 1).
  cgram[0] = rgb(64, 120, 208)     -- backdrop = deep sky blue
  m7.wrap = 2                      -- off the 1024x1024 plane -> transparent
  bg[1].scroll.y = -4096           -- and above the horizon we ARE off it
  color.op = "add"; color.addend = "fixed"; color.on.backdrop = true
  hdma(0, HORIZON, function(y)
    local a = y / HORIZON                       -- 0 at the top .. 1 at the horizon
    color.fixed = rgb(a * 90, a * 70, a * 40)   -- warm haze builds downward
  end)

  -- The floor. Below the horizon, rebuild the camera on every single scanline.
  hdma(HORIZON + 1, 223, function(y)
    local d = SCALE / (y - HORIZON)  -- THE perspective divide: scale ~ 1/distance
    m7.a, m7.d = d, d                -- d texels per pixel, both axes (b, c stay 0)
    m7.cx, m7.cy = 128, 0            -- pin screen centre to texel column 128 —
                                     -- exactly where the texture paints its road
    m7.wrap = 0                      -- down here the plane tiles forever...
    bg[1].scroll.y = (t * SPEED) / d -- ...and slides by at a constant texel speed
                                     -- at every depth: /d undoes the row's own
                                     -- scale. That IS the driving.
  end)
  -- Near the horizon the tiling shows: ghost copies of the road converge on
  -- the vanishing point. Every SNES racer had this — most hid it under fog.
end
-- Try: move HORIZON, or SCALE = 64 to hug the ground
-- Try: a bend — in the floor hook add  bg[1].scroll.x = (223 - y) * (223 - y) / 600
-- Try: the namesake — drag a photo into assets, set bg[1].source = "its-name"
"#;

const GOLDEN: &str = "tests/fixtures/golden_tutorial_mode7_road.png";

/// Pixel-identical to road() in web/src/studio/demos/tutorials/mode7Road.ts:
/// asphalt + dashed yellow centreline + white edge lines + dirt shoulders +
/// mottled grass, road strip centred on texel column 128 (the Lua's m7.cx).
fn road() -> Vec<u8> {
    let (w, h) = (1024usize, 1024usize);
    let mut data = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) * 4;
            let (r, g, b) = if (124..132).contains(&x) && y % 32 < 18 {
                (240, 208, 48) // dashed yellow centreline
            } else if (34..40).contains(&x) || (216..222).contains(&x) {
                (232, 232, 232) // solid white edge lines
            } else if (28..228).contains(&x) {
                let m = ((x / 8 + y / 8) % 2) as u8 * 6;
                (88 + m, 88 + m, 94 + m) // asphalt, faintly mottled
            } else if (16..240).contains(&x) {
                (146, 108, 66) // dirt shoulders
            } else {
                let m = ((x / 16 + y / 16) % 2) as u8;
                (40 + m * 8, 116 + m * 16, 40 + m * 8) // grass
            };
            data[i] = r;
            data[i + 1] = g;
            data[i + 2] = b;
            data[i + 3] = 255;
        }
    }
    data
}

/// The t=1.0 / f=60 frame the golden ships.
fn render() -> Vec<u8> {
    let mut e = common::engine_with(
        &mut |e| common::add_m7(e, "road", road(), 1024, 1024),
        &[("main.lua", MAIN_SRC)],
    );
    let lt = e.frame(1.0, 60).unwrap();
    render_frame(&lt, e.memory())
}

#[test]
fn sky_above_the_horizon_differs_from_the_ground_below() {
    let fb = render();
    let sky = common::px(&fb, 128, 40);
    let ground = common::px(&fb, 128, 180);
    assert_eq!(sky[3], 255);
    assert!(
        sky[2] > sky[0] && sky[2] > 120,
        "above the horizon should be blue backdrop sky, got {sky:?}"
    );
    assert_ne!(sky, ground, "horizon split missing: sky == ground");
}

#[test]
fn asphalt_grey_near_centre_bottom() {
    // x=100, y=200 lands left of the centreline, right of the edge line:
    // u = d*(100-128)+128 with d = 128/104 -> texel ~93 = plain asphalt.
    let fb = render();
    let p = common::px(&fb, 100, 200);
    let (max, min) = (*p[..3].iter().max().unwrap(), *p[..3].iter().min().unwrap());
    assert!(
        max - min < 25 && (60..150).contains(&p[0]),
        "expected mid-grey asphalt at (100,200), got {p:?}"
    );
}

#[test]
fn grass_beyond_the_shoulder() {
    // x=8, y=215: u wraps past the plane's left edge into far-side grass.
    let fb = render();
    let p = common::px(&fb, 8, 215);
    assert!(
        p[1] > p[0] + 30 && p[1] > p[2] + 30,
        "expected green grass at (8,215), got {p:?}"
    );
}

#[test]
fn centreline_is_dashed() {
    // m7.cx pins screen column 128 to texel column 128 — the dash column — so
    // walking down the centre must cross both yellow dashes and asphalt gaps.
    let fb = render();
    let (mut yellow, mut gap) = (false, false);
    for y in 110..224 {
        let p = common::px(&fb, 128, y);
        if p[0] > 180 && p[1] > 140 && p[2] < 100 {
            yellow = true;
        } else if p[0] > 60 && p[0] < 150 {
            gap = true;
        }
    }
    assert!(yellow, "no yellow dash found down screen column 128");
    assert!(gap, "no gap between dashes down screen column 128");
}

#[test]
fn tutorial_mode7_road_matches_golden_png() {
    assert!(Path::new(GOLDEN).exists());
    let actual = render();
    let expected = common::decode_png(GOLDEN);
    assert_eq!(actual.len(), expected.len());
    assert!(
        actual == expected,
        "mode7-road framebuffer differs from golden PNG"
    );
}

#[test]
#[ignore = "regenerates the committed mode7-road golden PNG"]
fn regen_golden_tutorial_mode7_road() {
    common::write_png(GOLDEN, &render());
}
