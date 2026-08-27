//! Tutorial toy 1/10 :: first-light — the Rust mirror of
//! web/src/studio/demos/tutorials/firstLight.ts (the Lua is verbatim; the
//! golden PNG proves the frame the studio ships).
mod common;

use ppu_core::render_frame;
use std::path::Path;

/// Byte-identical to MAIN_SRC in web/src/studio/demos/tutorials/firstLight.ts.
const MAIN_SRC: &str = r#"-- ppu.toys tutorial 1/10 :: first-light — registers, the backdrop, brightness, hdma
-- A register is a little memory slot on the picture chip: write a number into
-- it and the picture changes. That is all driving a PPU ever is. This whole
-- sunrise is three registers — no images anywhere:
--   cgram[0]     the backdrop colour: what shows where nothing else drew
--   brightness   the master fade, 0 (black) .. 15 (full) - INIDISP
--   color.fixed  the fixed colour (COLDATA), here ADDed onto the backdrop
-- frame(t, f) runs every frame: t = seconds, f = frame number. Edits re-run
-- live, and apply_pokes() replays your inspector tweaks (see pokes.lua).
function frame(t, f)
  apply_pokes()
  brightness = 15
  cgram[0] = hsl(268 + sin(t / 7) * 14, 0.55, 0.24)  -- pre-dawn violet, drifting

  -- Colour math: ADD a fixed colour onto the backdrop, every pixel.
  color.op = "add"; color.addend = "fixed"; color.on.backdrop = true

  -- hdma(y0, y1, fn) runs fn once per scanline y, top to bottom, after
  -- frame() returns. A register write inside lands on that line ONLY — one
  -- backdrop colour fans out into 224. This is the classic SNES gradient.
  local horizon = 150
  hdma(0, 223, function(y)
    if y < horizon then
      local a = y / horizon                        -- 0 up top .. 1 at the horizon
      color.fixed = rgb(a * a * 250, a * a * a * 165, a * 50)  -- dawn glow builds down the sky
      brightness = 11 + floor(a * 4)               -- night still hangs at the top
      if y >= horizon - 4 then brightness = 15 end -- first light: the sun's edge
    else
      local d = (y - horizon) / 73                 -- 0 at the horizon .. 1 at the bottom
      local g = (1 - d) * (1 - d) * (0.7 + 0.3 * (y % 2))    -- odd lines shimmer
      color.fixed = rgb(g * 220, g * 100, g * 45 + 15)       -- the sky reflects off the water
      brightness = 9 - floor(d * 4)                -- and the water fades into the dark
    end
  end)
end
-- Try: move horizon, swap "add" for "sub", or speed up the sky: sin(t / 7) -> sin(t)
"#;

const GOLDEN: &str = "tests/fixtures/golden_tutorial_first_light.png";

/// The t=1.0 / f=60 frame — asset-free, so the engine closure binds nothing.
fn render() -> Vec<u8> {
    let mut e = common::engine_with(&mut |_| {}, &[("main.lua", MAIN_SRC)]);
    let lt = e.frame(1.0, 60).unwrap();
    render_frame(&lt, e.memory())
}

#[test]
fn backdrop_is_not_black() {
    let fb = render();
    let p = common::px(&fb, 128, 80);
    assert_eq!(p[3], 255);
    assert_ne!(&p[..3], &[0, 0, 0], "backdrop pixel was black");
}

#[test]
fn per_scanline_coldata_builds_a_sky_gradient() {
    let fb = render();
    let top = common::px(&fb, 128, 8);
    let horizon = common::px(&fb, 128, 145);
    assert_ne!(
        top, horizon,
        "hdma COLDATA writes did not vary per scanline"
    );
    assert!(
        horizon[0] > top[0] + 60,
        "dawn glow should be much redder at the horizon: top {top:?} vs horizon {horizon:?}"
    );
}

#[test]
fn water_below_the_horizon_is_dimmer_than_the_sky() {
    let fb = render();
    let sky = common::px(&fb, 128, 145);
    let water = common::px(&fb, 128, 200);
    let lum = |p: &[u8]| p[..3].iter().map(|&c| c as u32).sum::<u32>();
    assert!(
        lum(water) < lum(sky),
        "per-scanline brightness should dim the water: sky {sky:?} vs water {water:?}"
    );
}

#[test]
fn first_light_matches_golden_png() {
    assert!(Path::new(GOLDEN).exists());
    let actual = render();
    let expected = common::decode_png(GOLDEN);
    assert_eq!(actual.len(), expected.len());
    assert!(
        actual == expected,
        "first-light framebuffer differs from golden PNG"
    );
}

#[test]
#[ignore = "regenerates the committed first-light golden PNG"]
fn regen_golden_first_light() {
    let fb = render();
    common::write_png(GOLDEN, &fb);
}
