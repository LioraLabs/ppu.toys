//! stage-lights tutorial toy (6/10): two hdma-driven window irises, XOR combine
//! logic, and fixed-colour subtract outside the colour window. Lua + asset are
//! byte-identical mirrors of web/src/studio/demos/tutorials/stageLights.ts.
mod common;

use ppu_core::{render_frame, LuaEngine, HEIGHT, WIDTH};
use std::path::Path;

const GOLDEN: &str = "tests/fixtures/golden_tutorial_stage_lights.png";

// Verbatim mirror of MAIN_SRC in stageLights.ts.
const MAIN_SRC: &str = r#"-- ppu.toys :: stage-lights (6/10 · two hdma spotlights, window combine logic, colour math)
-- Lesson 6 of the tutorial arc (after cavern-camera, before split-screen).
--
-- Three ideas compose here:
--   * A window is a per-scanline SPAN: win.w1/win.w2 are just [lo, hi] columns,
--     so an hdma that rewrites them every scanline can trace any shape.
--   * Combine logic (OR/AND/XOR/XNOR) folds the two windows into ONE region.
--   * Colour math blends each main-screen pixel with an addend (the sub screen
--     or a fixed colour); color.region says WHERE, relative to the colour window.
-- The show: two spotlight irises sweep the stage. Outside the beams the scene is
-- darkened by fixed-colour SUBTRACT (unlike the spotlight demo's clip-to-black,
-- the dark stage stays faintly readable). The windows are XOR-combined, so where
-- the beams cross, the overlap is carved back out of the light.
--
-- Setup stage: one dma copies the stage set into VRAM, once, at compile
-- (parallax-skyline, lesson 2, tells the full story).
local stage = dma("stage", { char = 0x1000, map = 0x0000 })
function frame(t, f)
  apply_pokes()
  mode = 1; brightness = 15
  bg[1].char_base = stage.char; bg[1].map_base = stage.map
  screen.main.bg1 = true    -- the stage, alone on the main screen
  screen.main.bg2 = false; screen.main.bg3 = false   -- power-on defaults ALL layers on: drop the rest
  screen.main.bg4 = false; screen.main.obj = false

  -- The COLOUR window follows BOTH windows. XOR = lit where exactly ONE beam is:
  -- watch the dark lens where the beams cross.
  win.color.w1 = true
  win.color.w2 = true
  win.color.combine = "XOR"

  -- Colour math, gated by the colour window. "outside" = math only where NO beam
  -- lands, so the beams stay untouched and everything else is pulled down.
  color.op = "sub"                  -- main - addend, floored at black
  color.addend = "fixed"            -- addend = the fixed colour, not the sub screen
  color.fixed = rgb(96, 96, 136)    -- blue-heavy subtract: the dark leans warm
  color.on.bg1 = true               -- apply to BG1 pixels (the whole stage)
  color.region = "outside"

  -- Two iris centres. Light 1 hovers near the performer; light 2 sweeps wide.
  local r = 48
  local cx1 = floor(128 + sin(t * 3.1) * 50)
  local cx2 = floor(128 + sin(t * 1.35) * 64)
  local cy1 = floor(134 + sin(t * 0.9) * 8)
  local cy2 = floor(134 - sin(t * 0.9) * 8)
  -- hdma runs this per scanline: each window's span is its circle's chord at
  -- row y (an empty span is lo > hi — that is how a window says "nothing here").
  hdma(0, 223, function(y)
    local d1 = r*r - (y - cy1) * (y - cy1)
    if d1 < 0 then
      win.w1.lo = 1; win.w1.hi = 0
    else
      local hw = floor(sqrt(d1))
      win.w1.lo = cx1 - hw; win.w1.hi = cx1 + hw
    end
    local d2 = r*r - (y - cy2) * (y - cy2)
    if d2 < 0 then
      win.w2.lo = 1; win.w2.hi = 0
    else
      local hw = floor(sqrt(d2))
      win.w2.lo = cx2 - hw; win.w2.hi = cx2 + hw
    end
  end)
  -- Try: combine "OR" (crossed beams merge) or "AND" (only the overlap is lit);
  --      slow light 2 (t * 0.4); tint instead of dim — color.region = "inside",
  --      color.op = "add", color.fixed = rgb(64, 48, 0) warms the beams.
end
"#;

// 9-colour stage palette — mirror of STAGE_PAL in stageLights.ts.
const STAGE_PAL: [(u8, u8, u8); 10] = [
    (0, 0, 0),      // 0 unused — the asset is fully opaque
    (40, 48, 88),   // 1 back-wall fold, dark
    (56, 64, 112),  // 2 back-wall fold, lit
    (120, 24, 40),  // 3 curtain red, lit
    (88, 16, 32),   // 4 curtain red, shadow
    (200, 160, 64), // 5 gold valance trim
    (136, 88, 48),  // 6 floorboard, lit
    (112, 72, 40),  // 7 floorboard, dark
    (64, 40, 24),   // 8 plank seam
    (24, 16, 32),   // 9 performer silhouette
];

/// Mirror of stageIndex() in stageLights.ts — integer math only.
fn stage_index(x: i32, y: i32) -> usize {
    let (hx, hy) = (x - 128, y - 118);
    if hx * hx + hy * hy <= 100 {
        return 9; // head, r = 10
    }
    if (128..=170).contains(&y) && (x - 128).abs() <= 8 {
        return 9; // torso
    }
    if (171..=204).contains(&y) && ((x - 122).abs() <= 3 || (x - 134).abs() <= 3) {
        return 9; // legs
    }
    if y < 20 {
        return if (x / 8) % 2 == 1 { 4 } else { 3 }; // top valance folds
    }
    if y < 24 {
        return 5; // gold trim under the valance
    }
    if y < 176 && !(28..228).contains(&x) {
        return if (x / 8) % 2 == 1 { 4 } else { 3 }; // wing curtains
    }
    if y >= 176 {
        let shift = (y - 176) / 12 * 12;
        if (x + shift) % 24 < 2 {
            return 8; // plank seam
        }
        return if ((x + shift) / 24) % 2 == 1 { 7 } else { 6 };
    }
    if (x / 16) % 2 == 1 {
        2
    } else {
        1
    } // back-wall drape folds
}

fn stage_rgba() -> Vec<u8> {
    let mut data = vec![0u8; WIDTH * HEIGHT * 4];
    for y in 0..HEIGHT as i32 {
        for x in 0..WIDTH as i32 {
            let (r, g, b) = STAGE_PAL[stage_index(x, y)];
            let i = (y as usize * WIDTH + x as usize) * 4;
            data[i] = r;
            data[i + 1] = g;
            data[i + 2] = b;
            data[i + 3] = 255;
        }
    }
    data
}

fn engine(src: &str) -> LuaEngine {
    common::engine_with(
        &mut |e| common::add_bg(e, "stage", stage_rgba(), WIDTH as u32, HEIGHT as u32, 4),
        &[("main.lua", src)],
    )
}

fn render(src: &str) -> Vec<u8> {
    let mut e = engine(src);
    let lt = e.frame(1.0, 60).unwrap();
    render_frame(&lt, e.memory())
}

/// Same scene with the math enable dropped: what an unlit == undarkened frame
/// looks like (the glow-demo baseline trick).
fn baseline() -> Vec<u8> {
    render(&MAIN_SRC.replace("color.on.bg1 = true", "color.on.bg1 = false"))
}

fn sum(p: &[u8]) -> u32 {
    p[0] as u32 + p[1] as u32 + p[2] as u32
}

// The t=1.0 iris centres, from the Lua's own formulas (floor(128+sin(3.1)*50)
// etc.): C1 = (130, 140), C2 = (190, 127), r = 48.
fn centres() -> ((i32, i32), (i32, i32)) {
    let t = 1.0f64;
    let cx1 = (128.0 + (t * 3.1).sin() * 50.0).floor() as i32;
    let cx2 = (128.0 + (t * 1.35).sin() * 64.0).floor() as i32;
    let cy1 = (134.0 + (t * 0.9).sin() * 8.0).floor() as i32;
    let cy2 = (134.0 - (t * 0.9).sin() * 8.0).floor() as i32;
    ((cx1, cy1), (cx2, cy2))
}

#[test]
fn beams_stay_bright_and_the_rest_of_the_stage_is_darkened() {
    let fb = render(MAIN_SRC);
    let base = baseline();
    // (100, 122): inside beam 1 only -> untouched by math (== the no-math frame).
    assert_eq!(
        common::px(&fb, 100, 122),
        common::px(&base, 100, 122),
        "inside beam 1 should be unlit-identical"
    );
    // (190, 109): beam 2's centre -> also untouched (proves w2 reaches the compositor).
    assert_eq!(
        common::px(&fb, 190, 109),
        common::px(&base, 190, 109),
        "inside beam 2 should be unlit-identical"
    );
    // (8, 60): the wing curtain, outside both beams -> subtract pulled it down.
    assert!(
        sum(common::px(&fb, 8, 60)) < sum(common::px(&base, 8, 60)),
        "outside the beams the stage should be darker than the no-math frame"
    );
}

#[test]
fn xor_carves_the_overlap_where_the_beams_cross() {
    // (160, 115) sits inside BOTH chords at t=1.0 (checked below via the
    // scanline bytes), so XOR excludes it -> darkened like the outside.
    let fb = render(MAIN_SRC);
    let base = baseline();
    assert!(
        sum(common::px(&fb, 160, 115)) < sum(common::px(&base, 160, 115)),
        "XOR should carve the beam overlap back out of the light"
    );
    // Flip the one combine-logic line to OR and the same pixel is lit.
    let or_fb =
        render(&MAIN_SRC.replace("win.color.combine = \"XOR\"", "win.color.combine = \"OR\""));
    assert_eq!(
        common::px(&or_fb, 160, 115),
        common::px(&base, 160, 115),
        "under OR the overlap should merge into the light"
    );
}

/// The hdma really traces BOTH irises: per-scanline window bytes match the
/// chord math the Lua performs, recomputed here from the same formulas.
#[test]
fn window_scanlines_trace_both_iris_chords() {
    let mut e = engine(MAIN_SRC);
    let lt = e.frame(1.0, 60).unwrap();
    let bytes = ppu_core::window_scanline_bytes(&lt);
    let stride = ppu_core::WIN_SCANLINE_STRIDE;
    assert_eq!(bytes.len(), HEIGHT * stride);

    let ((cx1, cy1), (cx2, cy2)) = centres();
    assert_eq!((cx1, cy1), (130, 140));
    assert_eq!((cx2, cy2), (190, 127));

    // bytes layout per row: [WH0, WH1, WH2, WH3, W12SEL, ...]
    let w1 = |y: usize| (bytes[y * stride] as i32, bytes[y * stride + 1] as i32);
    let w2 = |y: usize| (bytes[y * stride + 2] as i32, bytes[y * stride + 3] as i32);

    // Widest chord at each centre row: [cx - r, cx + r].
    assert_eq!(w1(cy1 as usize), (cx1 - 48, cx1 + 48));
    assert_eq!(w2(cy2 as usize), (cx2 - 48, cx2 + 48));
    // Chord at an off-centre row matches floor(sqrt(r^2 - dy^2)).
    let hw = |cy: i32, y: i32| ((48 * 48 - (y - cy) * (y - cy)) as f64).sqrt().floor() as i32;
    assert_eq!(w1(115), (cx1 - hw(cy1, 115), cx1 + hw(cy1, 115)));
    assert_eq!(w2(115), (cx2 - hw(cy2, 115), cx2 + hw(cy2, 115)));
    // The XOR test's overlap pixel (160, 115) really is inside both chords.
    assert!(w1(115).0 <= 160 && 160 <= w1(115).1);
    assert!(w2(115).0 <= 160 && 160 <= w2(115).1);
    // Above both circles each window carries the empty span (lo > hi).
    assert!(w1(0).0 > w1(0).1, "row 0 should be an empty w1 span");
    assert!(w2(0).0 > w2(0).1, "row 0 should be an empty w2 span");
}

#[test]
fn stage_lights_matches_golden_png() {
    assert!(Path::new(GOLDEN).exists());
    let actual = render(MAIN_SRC);
    let expected = common::decode_png(GOLDEN);
    assert_eq!(actual.len(), expected.len());
    assert_eq!(actual, expected, "stage-lights differs from golden PNG");
}

#[test]
#[ignore = "regenerates the committed stage-lights golden PNG"]
fn regen_golden_stage_lights() {
    let fb = render(MAIN_SRC);
    common::write_png(GOLDEN, &fb);
}
