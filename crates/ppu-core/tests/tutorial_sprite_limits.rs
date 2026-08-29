//! Tutorial toy 10/10 :: sprite-limits — the Rust mirror of
//! web/src/studio/demos/tutorials/spriteLimits.ts (the Lua is verbatim; the
//! golden PNG proves the frame the studio ships).
mod common;

use ppu_core::{render_frame, render_frame_stats};
use std::path::Path;

/// Byte-identical to MAIN_SRC in web/src/studio/demos/tutorials/spriteLimits.ts.
const MAIN_SRC: &str = r#"-- ppu.toys tutorial 10/10 :: sprite-limits — why 90s games flickered
-- The PPU re-evaluates all 128 OAM sprites on EVERY scanline, under two
-- hard budgets:
--   RANGE: only the first 32 sprites whose Y covers the line are kept
--   TIME:  the kept sprites may fetch at most 34 8px-wide tile slivers
-- Anything past a budget simply does not draw on that line. Both events
-- latch into STAT77 ($213E) — open the Sprites inspector and watch the
-- RANGE OVER and TIME OVER badges light up while this frame renders.
-- The classic fix was never "use fewer sprites": rotate where evaluation
-- STARTS (obj.first) each frame, so a DIFFERENT set pays the budget every
-- frame — flicker, the fair alternative to permanent invisibility.
function frame(t, f)
  apply_pokes()
  mode = 1; brightness = 15
  screen.main.obj = true   -- sprites only: no BG is designated (TM)
  local ROTATE = true   -- flip to false: the SAME sprites vanish forever
  obj.char_base = 0x4000
  obj.size_sel = 1      -- OBSEL pair 1: small = 8x8, large = 32x32
  -- solid 4bpp tiles 0..63 (colour index 1) so every sprite is a filled square
  for tn = 0, 63 do
    local base = 0x4000 + tn * 16
    for y = 0, 7 do vram[base + y] = 0x00ff end
  end
  cgram[0] = rgb(16, 16, 32)                 -- backdrop: dark navy
  cgram[128 + 1]      = rgb(80, 220, 100)    -- pal 0: control row, green
  cgram[128 + 16 + 1] = rgb(255, 168, 40)    -- pal 1: dense band, amber
  cgram[128 + 32 + 1] = rgb(255, 232, 140)   -- pal 2: dense band, pale amber
  cgram[128 + 48 + 1] = rgb(90, 180, 255)    -- pal 3: big-sprite row, blue

  -- ROW A, sprites 0..7 — the CONTROL. Eight 8x8 sprites on one line: under
  -- both budgets, so this row never loses a sprite. Compare the rows below
  -- against it.
  for i = 0, 7 do
    obj[i].tile = 0; obj[i].pal = 0
    obj[i].x = 16 + i * 30; obj[i].y = 32
    obj[i].on = true
  end

  -- ROW B, sprites 8..47 — the RANGE overflow. Forty 8x8 sprites all share
  -- scanlines 96..103. 40 > 32, so on every one of those lines the last 8 in
  -- evaluation order are dropped: the band comes up short of the right edge.
  -- Columns alternate two shades so you can count exactly which survive.
  for i = 0, 39 do
    local s = 8 + i
    obj[s].tile = 0; obj[s].pal = 1 + i % 2
    obj[s].x = 4 + i * 6; obj[s].y = 96
    obj[s].on = true
  end

  -- ROW C, sprites 48..57 — the TIME overflow, a different budget entirely.
  -- Only ten sprites, nowhere near the 32 cap — but each large 32x32 sprite
  -- spans 4 tile slivers per line, so 10 * 4 = 40 slivers > 34. The PPU keeps
  -- the evaluation-order prefix that fits (8 sprites = 32 slivers; a 9th
  -- would need 36) and the last two big squares vanish with the sprite
  -- budget barely touched. Big sprites are cheap in OAM, expensive in fetch.
  for i = 0, 9 do
    local s = 48 + i
    obj[s].tile = 0; obj[s].pal = 3
    obj[s].large = true
    obj[s].x = 4 + i * 24; obj[s].y = 150
    obj[s].on = true
  end

  -- The mitigation. obj.first sets where OAM evaluation starts (OAMADD
  -- priority rotation); the budgets still bite, but a different run of
  -- sprites pays each frame. With ROTATE = false the gaps freeze: the same
  -- sprites are invisible on every frame, forever. With true the gaps crawl
  -- through the rows at 60Hz — flicker shares the pain.
  if ROTATE then obj.first = f % 58 else obj.first = 0 end
end
-- Try: flip ROTATE and watch the gaps freeze; widen ROW B's loop to 47 and
-- more columns die; set large = true in ROW B too — 40 * 4 = 160 slivers
-- slams the 34-tile budget on top of the 32-sprite cap.
"#;

const GOLDEN: &str = "tests/fixtures/golden_tutorial_sprite_limits.png";

/// Frame at `f` with per-frame OBJ overflow stats. Asset-free: the engine
/// closure binds nothing.
fn render_stats(f: u32) -> (Vec<u8>, ppu_core::ObjOverflow) {
    let mut e = common::engine_with(&mut |_| {}, &[("main.lua", MAIN_SRC)]);
    let lt = e.frame(1.0, f).unwrap();
    render_frame_stats(&lt, e.memory())
}

/// The shipped t=1.0 / f=60 frame through the plain render path (the golden).
fn render() -> Vec<u8> {
    let mut e = common::engine_with(&mut |_| {}, &[("main.lua", MAIN_SRC)]);
    let lt = e.frame(1.0, 60).unwrap();
    render_frame(&lt, e.memory())
}

#[test]
fn dense_band_trips_range_over_and_big_row_trips_time_over() {
    let (_fb, ov) = render_stats(60);
    assert!(
        ov.range_over,
        "40-sprite band must exceed the 32-sprite cap"
    );
    assert!(
        ov.time_over,
        "10 large 32x32 sprites (40 slivers) must exceed the 34-tile cap"
    );
    assert!(ov.max_sprites > 32);
}

#[test]
fn control_row_never_drops_a_sprite() {
    let fb = render();
    // Row A: 8 sprites at x = 16 + i*30, y = 32 — every one draws green.
    for i in 0..8 {
        let p = common::px(&fb, 16 + i * 30 + 4, 36);
        assert!(
            p[1] > 150 && p[0] < 150,
            "control sprite {i} missing: {p:?}"
        );
    }
}

#[test]
fn range_cap_drops_the_band_tail_but_keeps_the_head() {
    // f=60 -> obj.first = 2: row B eval order is 8..47, so sprites 40..47
    // (band x >= 196) are range-dropped while the head survives.
    let fb = render_stats(60).0;
    let kept = common::px(&fb, 100, 100);
    assert!(kept[0] > 180, "band head pixel missing: {kept:?}");
    let dropped = common::px(&fb, 230, 100);
    assert_eq!(
        &dropped[..3],
        common::px(&fb, 250, 20).get(..3).unwrap(),
        "range-dropped region should show the backdrop"
    );
}

#[test]
fn time_cap_drops_the_last_two_big_sprites() {
    // Row C keeps 8 * 4 = 32 slivers; sprites 56 (x 196..228) and 57
    // (x 220..252) are time-dropped.
    let fb = render_stats(60).0;
    let kept = common::px(&fb, 100, 160);
    assert!(kept[2] > 180, "big-row kept pixel missing: {kept:?}");
    let dropped = common::px(&fb, 240, 160);
    assert_eq!(
        &dropped[..3],
        common::px(&fb, 250, 20).get(..3).unwrap(),
        "time-dropped region should show the backdrop"
    );
}

#[test]
fn rotating_obj_first_changes_which_sprites_drop() {
    // f=60 (start 2) drops the band tail; f=20 (start 20) drops sprites
    // 12..19 near the band head instead. Same budgets, different victims.
    assert!(
        render_stats(60).0 != render_stats(20).0,
        "OAM rotation must change the dropped set across frames"
    );
}

#[test]
fn sprite_limits_matches_golden_png() {
    assert!(Path::new(GOLDEN).exists());
    let actual = render();
    let expected = common::decode_png(GOLDEN);
    assert_eq!(actual.len(), expected.len());
    assert!(
        actual == expected,
        "sprite-limits framebuffer differs from golden PNG"
    );
}

#[test]
#[ignore = "regenerates the committed sprite-limits golden PNG"]
fn regen_golden_sprite_limits() {
    let fb = render();
    common::write_png(GOLDEN, &fb);
}
