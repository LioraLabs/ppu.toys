//! Tutorial toy 9/10 :: extbg-direct-color — the Rust mirror of
//! web/src/studio/demos/tutorials/extbgDirectColor.ts (the Lua is verbatim;
//! the golden PNG proves the frame the studio ships).
mod common;

use ppu_core::render_frame;
use std::path::Path;

/// Byte-identical to MAIN_SRC in web/src/studio/demos/tutorials/extbgDirectColor.ts.
const MAIN_SRC: &str = r#"-- ppu.toys tutorial 9/10 :: extbg-direct-color — Mode 7's two deep cuts, together
-- (1) EXTBG (SETINI.6): each Mode 7 pixel's bit 7 becomes a per-pixel PRIORITY.
--     The single floor splits into two levels with sprites sandwiched between,
--     front to back:  OBJ3 · M7-high · OBJ2 · OBJ1 · M7-low · OBJ0 · backdrop
--     This is how real carts build pillars a sprite passes BEHIND.
-- (2) Direct colour (CGWSEL.0): the 8-bit pixel value IS the colour — BGR233
--     expanded to BGR555, CGRAM never consulted (see mode7-extbg and
--     direct-color in the demo shelf for each trick alone).
-- Together: EXTBG steals bit 7, so the LOW 7 bits expand instead —
-- 3 bits red, 3 bits green, 1 (dim) bit blue. Every byte poked below is
-- simultaneously a colour and a priority. The pixel values ARE the lesson.
function frame(t, f)
  apply_pokes()
  mode = 7; brightness = 15
  -- EXTBG splits the ONE plane into two designatable halves: BG1 carries the
  -- low-priority pixels, BG2 the high ones — so both go on the main screen.
  screen.main.bg1 = true; screen.main.bg2 = true; screen.main.obj = true
  m7.a, m7.d = 1, 1
  m7.extbg = true                        -- bit 7 = priority (SETINI.6)
  direct_color = true                    -- low 7 bits = colour (CGWSEL.0)

  -- Poke tile v solid with value v (tile number == pixel byte, once per value).
  local done = {}
  local function ink(v)
    if not done[v] then
      done[v] = true
      for fy = 0, 7 do for fx = 0, 7 do m7pixel(v, fx, fy, v) end end
    end
    return v
  end

  -- The ground and sky: a smooth two-axis colour field with NO palette. Watch
  -- the packing — this line is the whole direct-colour trick:
  for ty = 0, 27 do
    m7.map[ty] = {}
    for tx = 0, 31 do
      local r, g, b
      if ty < 10 then                    -- dusk sky: blue base, red rising to the horizon
        r, g, b = floor(ty / 3), 0, 1
      else                               -- floor: red follows x, green follows depth
        r = 1 + floor(tx * 6 / 31)
        g = 2 + floor((ty - 10) * 5 / 17)
        b = 0
      end
      local idx = r + g * 8 + b * 64     -- BGR233 packing: r,g in 0..7, b in 0..1 (bit 7 is spoken for)
      m7.map[ty][tx] = ink(idx)          -- bit 7 clear -> LOW floor: sprites float above
    end
  end

  -- The colonnade: same bytes + 0x80. Bit 7 lifts these pixels ABOVE OBJ prio
  -- 2, so the sprite slides behind them — no windows, no second layer.
  local LIT, SHADE, BEAM = 0x80 + 47, 0x80 + 21, 0x80 + 29
  local cols = {4, 15, 26}
  for i = 1, 3 do
    for ty = 3, 15 do
      m7.map[ty][cols[i]] = ink(LIT)     -- sunlit face (r=7 g=5)
      m7.map[ty][cols[i] + 1] = ink(SHADE) -- shaded face (r=5 g=2)
    end
  end
  for tx = 3, 28 do m7.map[2][tx] = ink(BEAM) end -- the lintel across the top

  -- The lantern: a 32x32 sprite at prio 2 — under M7-high, over M7-low.
  -- Sprites still read CGRAM: direct colour is strictly a BG affair.
  cgram[128 + 1] = rgb(255, 220, 80)
  obj.char_base = 0x4000
  obj.size_sel = 1                       -- large pair = 32x32
  for row = 0, 3 do                      -- fill the 4x4 tile block solid (index 1)
    for col = 0, 3 do
      local base = 0x4000 + (row * 16 + col) * 16
      for y = 0, 7 do vram[base + y] = 0x00ff end
    end
  end
  obj[0].tile = 0; obj[0].pal = 0; obj[0].prio = 2
  obj[0].large = true
  obj[0].x = 64 + floor(sin(t * 0.7) * 72)  -- drift through the colonnade...
  obj[0].y = 84 + floor(sin(t * 3) * 4)     -- ...with a gentle bob
  obj[0].on = true
end
-- Try: LIT = 47 (clear its bit 7 — that pillar face drops below the sprite),
-- obj[0].prio = 3 (the lantern rides over everything), or
-- direct_color = false (CGRAM is empty, so the whole floor goes black).
"#;

const GOLDEN: &str = "tests/fixtures/golden_tutorial_extbg_direct_color.png";

/// The t=1.0 / f=60 frame — asset-free, so the engine closure binds nothing.
fn render_src(src: &str) -> Vec<u8> {
    let mut e = common::engine_with(&mut |_| {}, &[("main.lua", src)]);
    let lt = e.frame(1.0, 60).unwrap();
    render_frame(&lt, e.memory())
}

fn render() -> Vec<u8> {
    render_src(MAIN_SRC)
}

fn is_lantern(p: &[u8]) -> bool {
    // cgram[129] = rgb(255, 220, 80) -> [255, 222, 82, 255] after 5-bit expand.
    p[0] > 200 && p[1] > 180 && p[2] < 120 && p[3] == 255
}

/// At t=1.0 the sprite spans x 110..142, y 84..116; the centre pillar (map
/// columns 15/16) spans x 120..136, rows 24..128 — the sprite straddles it.
#[test]
fn sprite_rides_the_low_floor_but_ducks_behind_the_high_pillar() {
    let fb = render();
    // Left sliver of the sprite over LOW floor -> lantern yellow shows.
    assert!(
        is_lantern(common::px(&fb, 114, 100)),
        "sprite must show over the low floor: {:?}",
        common::px(&fb, 114, 100)
    );
    // x=128 is the pillar's SHADE face (byte 0x80+21): high priority covers the
    // sprite. Low 7 bits (r=5, g=2, b=0) expand to exactly [165, 66, 0].
    assert_eq!(
        common::px(&fb, 128, 100),
        &[165, 66, 0, 255],
        "high-priority pillar face must cover the sprite"
    );
    // With EXTBG off the whole plane drops to one level -> the sprite overlays
    // the pillar too.
    let flat = render_src(&MAIN_SRC.replace("m7.extbg = true", "m7.extbg = false"));
    assert!(
        is_lantern(common::px(&flat, 128, 100)),
        "EXTBG off should flat-overlay the sprite everywhere"
    );
}

#[test]
fn floor_is_direct_colour_not_cgram() {
    let fb = render();
    // (60, 150): map tile (tx=7, ty=18) -> r=2, g=4, b=0 -> byte 34. Direct
    // colour expands the low 7 bits: r5=8, g5=16 -> [66, 132, 0].
    assert_eq!(common::px(&fb, 60, 150), &[66, 132, 0, 255]);
    // CGRAM entry 34 was never written — the colour cannot have come from it.
    let mut e = common::engine_with(&mut |_| {}, &[("main.lua", MAIN_SRC)]);
    e.frame(1.0, 60).unwrap();
    assert_eq!(e.memory().cgram[34], 0, "BG CGRAM must stay untouched");
    // Direct colour off -> the same pixel reads empty CGRAM -> black.
    let off = render_src(&MAIN_SRC.replace("direct_color = true", "direct_color = false"));
    assert_eq!(
        common::px(&off, 60, 150),
        &[0, 0, 0, 255],
        "without the bypass the empty palette shows"
    );
}

#[test]
fn floor_field_is_smooth_on_both_axes() {
    let fb = render();
    // Red rises with x along one floor row (away from pillars and sprite).
    assert!(
        common::px(&fb, 250, 180)[0] > common::px(&fb, 70, 180)[0],
        "red should rise with x on the floor"
    );
    // Green rises with depth down one clear column.
    assert!(
        common::px(&fb, 70, 220)[1] > common::px(&fb, 70, 130)[1],
        "green should rise with y on the floor"
    );
}

#[test]
fn extbg_direct_color_matches_golden_png() {
    assert!(Path::new(GOLDEN).exists());
    let actual = render();
    let expected = common::decode_png(GOLDEN);
    assert_eq!(actual.len(), expected.len());
    assert!(
        actual == expected,
        "extbg-direct-color framebuffer differs from golden PNG"
    );
}

#[test]
#[ignore = "regenerates the committed extbg-direct-color golden PNG"]
fn regen_golden_extbg_direct_color() {
    let fb = render();
    common::write_png(GOLDEN, &fb);
}
