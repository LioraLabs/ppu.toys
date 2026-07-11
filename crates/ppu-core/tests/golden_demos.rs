//! Flagship demo golden tests through the real Lua/importer/render pipeline.
use ppu_core::{render_frame, ImportBudget, LuaEngine, HEIGHT, WIDTH};
use std::path::Path;

const DUSK_GOLDEN: &str = "tests/fixtures/golden_dusk_parallax.png";
const MODE7_GOLDEN: &str = "tests/fixtures/golden_mode7_floor.png";
const OFFSET_GOLDEN: &str = "tests/fixtures/golden_offset_per_tile.png";
const MODE3_GOLDEN: &str = "tests/fixtures/golden_mode3_gradient.png";
const MODE0_GOLDEN: &str = "tests/fixtures/golden_mode0_bands.png";
const TRANSLUCENCY_GOLDEN: &str = "tests/fixtures/golden_translucency.png";
const SPOTLIGHT_GOLDEN: &str = "tests/fixtures/golden_spotlight.png";
const GLOW_GOLDEN: &str = "tests/fixtures/golden_glow.png";
const TM_MASK_GOLDEN: &str = "tests/fixtures/golden_tm_mask.png";
const SHADOW_GOLDEN: &str = "tests/fixtures/golden_shadow.png";
const SPRITE_STORM_GOLDEN: &str = "tests/fixtures/golden_sprite_storm.png";
const MOSAIC_GOLDEN: &str = "tests/fixtures/golden_mosaic.png";
const EXTBG_GOLDEN: &str = "tests/fixtures/golden_extbg.png";
const DIRECT_GOLDEN: &str = "tests/fixtures/golden_direct_color.png";

const DUSK_MAIN_SRC: &str = r#"-- ppu.toys :: dusk-parallax (Mode 1: parallax BG scroll + CGRAM colour-cycle + sprite)
-- Multi-file flagship: SPEED + dusk_palette() live in palette.lua. Chunks run in
-- tab order into ONE shared global scope; frame() resolves after all chunks, so
-- main.lua may reference palette.lua globals freely (main.lua is convention, not magic).
function frame(t, f)
  apply_pokes()
  mode = 1; brightness = 15
  bg[1].source = "sky";   bg[2].source = "hills"
  bg[2].map_base = 0x0800; bg[2].char_base = 0x4000
  bg[1].scroll.x = t * SPEED
  bg[2].scroll.x = t * SPEED * 3
  dusk_palette(t)
  obj[0].tile = 4; obj[0].pal = 0; obj[0].prio = 3; obj[0].x = 120; obj[0].y = 132 + sin(t*3) * 4
  obj.char_base = 0x6000; obj.sheet = "hero"; obj[0].on = true
end
"#;

const DUSK_PALETTE_SRC: &str = r#"-- dusk-parallax :: palette.lua — CGRAM colour-cycle ($40-$47), globals shared with main.lua
SPEED = 12
function dusk_palette(t)
  for i = 0, 7 do cgram[0x40 + i] = hsl((t*40 + i*12) % 360, 0.6, 0.5) end
end
"#;

/// The single-file concat of the flagship's USER chunks (main + palette) for the
/// multi-file parity golden. The pokes chunk is not part of this concat — it is
/// prepended separately by `demo_engine_files`, mirroring web tab order.
fn dusk_concat() -> String {
    format!("{DUSK_MAIN_SRC}\n{DUSK_PALETTE_SRC}")
}

const MODE7_SRC: &str = r#"-- ppu.toys :: mode7-floor (the namesake; per-scanline affine floor)
function frame(t, f)
  apply_pokes()
  mode = 7; brightness = 15; bg[1].source = "track"
  hdma(96, 223, function(y)
    local d = 64 / (y - 95)
    m7.a, m7.d = d, d
    m7.cx, m7.cy = 128, 0
    bg[1].scroll.y = (t*80) * d
  end)
end
"#;

const OFFSET_SRC: &str = r#"-- ppu.toys :: offset-per-tile (Mode 2: BG3 table drives per-column scroll)
function column_offset(col, dh, dv)
  local base = 0x0800
  bg[3].map_base = base
  local enable = 0x2000
  vram[base + col] = enable + (dh % 1024)
  vram[base + 32 + col] = enable + 0x8000 + (dv % 1024)
end

function frame(t, f)
  apply_pokes()
  mode = 2; brightness = 15
  bg[1].source = "ribbons"
  bg[1].char_base = 0x1000
  bg[3].map_base = 0x0800
  for col = 0, 31 do
    local wave = floor((sin((col + t * 8) / 3) + 1) * 4)
    column_offset(col, wave, col % 3)
  end
end
"#;

const MODE3_SRC: &str = r#"-- ppu.toys :: mode3-gradient (Mode 3: 8bpp 256-colour BG1 gradient)
function frame(t, f)
  apply_pokes()
  mode = 3; brightness = 15
  bg[1].source = "gradient"
  bg[1].char_base = 0x1000
end
"#;

const MODE0_SRC: &str = r#"-- ppu.toys :: mode0-bands (Mode 0: two 2bpp layers, per-layer CGRAM band)
function frame(t, f)
  mode = 0; brightness = 15
  bg[1].source = "mode0_bg1"
  bg[2].source = "mode0_bg2"; bg[2].map_base = 0x0400; bg[2].char_base = 0x2000
end
"#;

const TRANSLUCENCY_SRC: &str = r#"-- ppu.toys :: translucency (½-add glass panel over a scrolling BG)
function frame(t, f)
  apply_pokes()
  mode = 1; brightness = 15
  bg[1].source = "panel"                       -- the glass panel (main only)
  bg[2].source = "ribbons"; bg[2].char_base = 0x2000  -- scene, on main AND sub
  bg[2].map_base = 0x0800
  screen.main.bg1 = true; screen.main.bg2 = true      -- panel + scene on the main screen
  screen.main.bg3 = false; screen.main.bg4 = false; screen.main.obj = false
  screen.sub.bg2 = true    -- scene on the sub screen -> the addend under the glass
  color.op = "add"; color.half = true; color.on.bg1 = true  -- ½-add math on BG1 (the glass)
  color.addend = "sub"     -- addend = subscreen (not fixed colour)
end
"#;

const SPOTLIGHT_SRC: &str = r#"-- ppu.toys :: spotlight (per-scanline circular iris via the colour window)
function frame(t, f)
  apply_pokes()
  mode = 1; brightness = 15
  bg[1].source = "ribbons"
  screen.main.bg1 = true    -- BG1 only on the main screen
  screen.main.bg2 = false; screen.main.bg3 = false
  screen.main.bg4 = false; screen.main.obj = false
  win.color.w1 = true       -- COLOR window follows window 1
  win.color.combine = "OR"  -- COLOR window logic = OR
  -- clip-to-black = 01 (outside the window -> black); raw on purpose: CGWSEL
  -- bits 6-7 have no friendly field (color owns only addend/region)
  CGWSEL = 0x40
  -- iris: per scanline, window 1 spans [cx-hw, cx+hw] where hw traces a circle.
  local cx, cy, r = 128, 112, 70
  hdma(0, 223, function(y)
    local dy = y - cy
    local inside = r*r - dy*dy
    if inside < 0 then
      win.w1.lo = 1; win.w1.hi = 0   -- empty span (left > right) -> nothing inside
    else
      local hw = floor(sqrt(inside))
      win.w1.lo = cx - hw
      win.w1.hi = cx + hw
    end
  end)
end
"#;

const GLOW_SRC: &str = r#"-- ppu.toys :: additive-glow (fixed-colour add brightens BG1 toward warm)
function frame(t, f)
  apply_pokes()
  mode = 1; brightness = 15
  bg[1].source = "ribbons"
  screen.main.bg1 = true    -- BG1 only on the main screen
  screen.main.bg2 = false; screen.main.bg3 = false
  screen.main.bg4 = false; screen.main.obj = false
  color.op = "add"; color.on.bg1 = true   -- add at full strength (half stays off)
  color.addend = "fixed"    -- addend = the fixed colour, not the sub screen
  color.fixed = rgb(120, 60, 0)  -- warm glow added to every BG1 pixel
end
"#;

const TM_MASK_SRC: &str = r#"-- ppu.toys :: tm-mask (TM drops BG2 from the main screen)
function frame(t, f)
  mode = 0; brightness = 15
  bg[1].source = "mode0_bg1"
  bg[2].source = "mode0_bg2"; bg[2].map_base = 0x0400; bg[2].char_base = 0x2000
  TM = 0x01   -- BG1 only; BG2 is masked off the main screen
end
"#;

const SHADOW_SRC: &str = r#"-- ppu.toys :: shadow (subtractive fixed-colour darkens BG1)
function frame(t, f)
  mode = 1; brightness = 15
  bg[1].source = "ribbons"
  TM = 0x01
  CGADSUB = 0x81          -- subtract (bit7) + BG1 math-enable
  CGWSEL = 0x00           -- addend = COLDATA fixed colour
  COLDATA = rgb(120, 120, 120)
end
"#;

const SPRITE_STORM_SRC: &str = r#"-- ppu.toys :: sprite-storm (authentic OBJ flicker: >32 sprites on one band, OAM start rotates each frame)
function frame(t, f)
  apply_pokes()
  mode = 1; brightness = 15
  obj.char_base = 0x4000
  obj.size_sel = 7           -- small 16x32 (non-square), large 32x32
  -- solid 4bpp OBJ tiles (index 1) so large sprites fill fully
  for tn = 0, 63 do
    local base = 0x4000 + tn * 16
    for y = 0, 7 do vram[base + y] = 0x00ff end
  end
  cgram[0] = rgb(24, 16, 48)               -- backdrop
  for p = 0, 7 do cgram[128 + p * 16 + 1] = hsl(p * 44, 0.8, 0.55) end
  local N = 48
  for i = 0, N - 1 do
    obj[i].tile = 0; obj[i].pal = i % 8
    obj[i].x = 8 + (i * 15) % 232; obj[i].y = 96
    obj[i].large = (i % 12 == 0)           -- a few 32x32 among the 16x32 storm
    obj[i].on = true
  end
  obj.first = f % N                        -- rotate OAM eval start -> flicker
end
"#;

const MOSAIC_SRC: &str = r#"-- ppu.toys :: mosaic (BG1 pixelation; block size steps every 8 frames)
function frame(t, f)
  apply_pokes()
  mode = 3; brightness = 15
  bg[1].source = "ramp"
  bg[1].mosaic = true
  mosaic = floor(f / 8) % 16
end
"#;

const EXTBG_SRC: &str = r#"-- ppu.toys :: mode7-extbg (per-pixel floor priority; sprite between the two levels)
function frame(t, f)
  apply_pokes()
  mode = 7; brightness = 15
  m7.a, m7.d = 1, 1
  m7.extbg = true
  cgram[1] = rgb(216, 64, 64)          -- Mode 7 floor colour 1 = red
  cgram[128 + 1] = rgb(255, 255, 0)    -- OBJ pal0 idx1 = yellow
  for fy = 0, 7 do
    for fx = 0, 7 do
      m7pixel(1, fx, fy, 0x81)         -- high priority (bit7) + colour 1
      m7pixel(2, fx, fy, 0x01)         -- low priority + colour 1
    end
  end
  for ty = 0, 27 do
    m7.map[ty] = {}
    for tx = 0, 31 do m7.map[ty][tx] = (tx < 16) and 1 or 2 end
  end
  obj.char_base = 0x4000
  obj.size_sel = 1                     -- large pair = 32x32
  for row = 0, 3 do                    -- fill the 4x4 tile block solid (index 1)
    for col = 0, 3 do
      local base = 0x4000 + (row * 16 + col) * 16
      for y = 0, 7 do vram[base + y] = 0x00ff end
    end
  end
  obj[0].tile = 0; obj[0].pal = 0; obj[0].prio = 2
  obj[0].large = true                  -- 32x32
  obj[0].x = 112; obj[0].y = 88; obj[0].on = true
end
"#;

const DIRECT_SRC: &str = r#"-- ppu.toys :: direct-color (8bpp Mode 7, CGRAM bypass, smooth colour field)
function frame(t, f)
  apply_pokes()
  mode = 7; brightness = 15
  m7.a, m7.d = 1, 1
  direct_color = true
  local done = {}
  for ty = 0, 27 do
    m7.map[ty] = {}
    for tx = 0, 31 do
      local r = floor(tx * 7 / 31)
      local g = floor(ty * 7 / 27)
      local b = 1 + floor((tx + ty) * 2 / 58)
      local idx = r + g * 8 + b * 64
      m7.map[ty][tx] = idx
      if not done[idx] then
        done[idx] = true
        for fy = 0, 7 do for fx = 0, 7 do m7pixel(idx, fx, fy, idx) end end
      end
    end
  end
end
"#;

fn ramp() -> Vec<u8> {
    let mut data = vec![0u8; WIDTH * HEIGHT * 4];
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let i = (y * WIDTH + x) * 4;
            data[i] = ((x % 32) * 8) as u8;
            data[i + 1] = ((y % 32) * 8) as u8;
            data[i + 2] = 128;
            data[i + 3] = 255;
        }
    }
    data
}

fn sky() -> Vec<u8> {
    const W: usize = WIDTH;
    const H: usize = HEIGHT;
    const HORIZON: usize = 140;
    let mut data = vec![0u8; W * H * 4];
    for y in 0..H {
        for x in 0..W {
            let i = (y * W + x) * 4;
            if y >= HORIZON {
                data[i + 3] = 0;
                continue;
            }
            let dx = x as i32 - 192;
            let dy = y as i32 - 50;
            if dx * dx + dy * dy < 20 * 20 {
                data[i..i + 4].copy_from_slice(&[255, 226, 168, 255]);
                continue;
            }
            let t = y as f32 / HORIZON as f32;
            data[i] = 30 + (t * t * 210.0).round() as u8;
            data[i + 1] = 18 + (t * 70.0).round() as u8;
            data[i + 2] = 78 + (t * 52.0).round() as u8;
            data[i + 3] = 255;
        }
    }
    data
}

fn hills() -> Vec<u8> {
    const W: usize = WIDTH;
    const H: usize = HEIGHT;
    const TOP: usize = 138;
    let mut data = vec![0u8; W * H * 4];
    for y in 0..H {
        for x in 0..W {
            let i = (y * W + x) * 4;
            if y < TOP {
                data[i + 3] = 0;
                continue;
            }
            let stripe = (x / 16) % 2;
            let d = (y - TOP) as f32 / (H - TOP) as f32;
            data[i] = 18 + stripe as u8 * 10;
            data[i + 1] = 96 - (d * 46.0).round() as u8 + stripe as u8 * 12;
            data[i + 2] = 38 + stripe as u8 * 8;
            data[i + 3] = 255;
        }
    }
    data
}

fn hero() -> Vec<u8> {
    let (w, h) = (64usize, 8usize);
    let mut data = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) * 4;
            let cell = x / 8;
            data[i] = 255 - cell as u8 * 16;
            data[i + 1] = 200;
            data[i + 2] = cell as u8 * 24;
            data[i + 3] = 255;
        }
    }
    data
}

fn track() -> Vec<u8> {
    let (w, h) = (1024usize, 1024usize);
    let mut data = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let (cx, cy) = ((x / 8) % 8, (y / 8) % 8);
            let i = (y * w + x) * 4;
            data[i] = cx as u8 * 32;
            data[i + 1] = cy as u8 * 32;
            data[i + 2] = (((cx + cy) & 1) * 255) as u8;
            data[i + 3] = 255;
        }
    }
    data
}

fn ribbons() -> Vec<u8> {
    let mut data = vec![0u8; WIDTH * HEIGHT * 4];
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let i = (y * WIDTH + x) * 4;
            let band = ((x / 8) % 8) as u8;
            data[i] = 32 + band * 24;
            data[i + 1] = 40 + ((y / 8) % 8) as u8 * 24;
            data[i + 2] = 220 - band * 16;
            data[i + 3] = 255;
        }
    }
    data
}

fn panel() -> Vec<u8> {
    let mut data = vec![0u8; WIDTH * HEIGHT * 4];
    for y in 0..HEIGHT {
        let opaque = (80..160).contains(&y);
        for x in 0..WIDTH {
            let i = (y * WIDTH + x) * 4;
            if opaque {
                data[i..i + 4].copy_from_slice(&[80, 230, 255, 255]); // cyan glass
            } // else alpha 0
        }
    }
    data
}

fn gradient() -> Vec<u8> {
    let mut data = vec![0u8; WIDTH * HEIGHT * 4];
    for y in 0..HEIGHT {
        // top->bottom hue sweep; constant across x so unique tiles stay bounded.
        let r = (y * 255 / (HEIGHT - 1)) as u8;
        let g = ((HEIGHT - 1 - y) * 255 / (HEIGHT - 1)) as u8;
        for x in 0..WIDTH {
            let i = (y * WIDTH + x) * 4;
            data[i] = r;
            data[i + 1] = g;
            data[i + 2] = 128;
            data[i + 3] = 255;
        }
    }
    data
}

fn mode0_bg1() -> Vec<u8> {
    let mut data = vec![0u8; WIDTH * HEIGHT * 4];
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let i = (y * WIDTH + x) * 4;
            if (x / 8) % 2 == 0 {
                data[i..i + 4].copy_from_slice(&[40, 220, 90, 255]); // green
            } // else alpha 0 = transparent
        }
    }
    data
}

fn mode0_bg2() -> Vec<u8> {
    let mut data = vec![0u8; WIDTH * HEIGHT * 4];
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let i = (y * WIDTH + x) * 4;
            if (y / 8) % 2 == 0 {
                data[i..i + 4].copy_from_slice(&[220, 60, 200, 255]); // magenta
            }
        }
    }
    data
}

/// Empty pokes.lua chunk — mirrors `pokesToLua([])` / `EMPTY_POKES` from
/// web/src/studio/pokes/pokes.ts byte-for-byte. Demos that call apply_pokes()
/// as frame()'s first line need this no-op definition in scope; demos that
/// don't (the Rust-only MODE0/TM_MASK/SHADOW fixtures) simply never call it.
const EMPTY_POKES_SRC: &str = r#"-- pokes.lua · generated by the inspector — read-only.
-- Poke register/CGRAM values in the inspector to fill this in. To save a
-- configuration, copy apply_pokes() into your own file under a new name.
-- Hand-edits here are overwritten by the next poke.
function apply_pokes()
end
"#;

fn demo_engine_files(files: &[(&str, &str)]) -> LuaEngine {
    let mut e = LuaEngine::new();
    e.upload_asset("sky".into(), WIDTH as u32, HEIGHT as u32, sky());
    e.upload_asset("hills".into(), WIDTH as u32, HEIGHT as u32, hills());
    e.upload_asset("hero".into(), 64, 8, hero());
    e.upload_asset("track".into(), 1024, 1024, track());
    e.upload_asset("ribbons".into(), WIDTH as u32, HEIGHT as u32, ribbons());
    e.upload_asset("gradient".into(), WIDTH as u32, HEIGHT as u32, gradient());
    e.upload_asset("mode0_bg1".into(), WIDTH as u32, HEIGHT as u32, mode0_bg1());
    e.upload_asset("mode0_bg2".into(), WIDTH as u32, HEIGHT as u32, mode0_bg2());
    e.upload_asset("panel".into(), WIDTH as u32, HEIGHT as u32, panel());
    e.upload_asset("ramp".into(), WIDTH as u32, HEIGHT as u32, ramp());
    let mut chunks = Vec::with_capacity(files.len() + 1);
    chunks.push(("pokes.lua", EMPTY_POKES_SRC));
    chunks.extend_from_slice(files);
    e.set_sources(&chunks).unwrap();
    e
}

fn demo_engine(src: &str) -> LuaEngine {
    demo_engine_files(&[("source", src)])
}

/// One RGBA pixel at (x, y) in a WIDTH*HEIGHT framebuffer.
fn px(fb: &[u8], x: usize, y: usize) -> &[u8] {
    &fb[(y * WIDTH + x) * 4..][..4]
}

fn render_storm(f: u32) -> (Vec<u8>, ppu_core::ObjOverflow) {
    let mut e = demo_engine(SPRITE_STORM_SRC);
    let lt = e.frame(1.0, f).unwrap();
    ppu_core::render_frame_stats(&lt, e.memory())
}

fn render_demo(src: &str) -> (Vec<u8>, LuaEngine) {
    let mut e = demo_engine(src);
    let lt = e.frame(1.0, 60).unwrap();
    let fb = render_frame(&lt, e.memory());
    (fb, e)
}

fn decode_png(path: &str) -> Vec<u8> {
    let decoder = png::Decoder::new(std::fs::File::open(path).unwrap());
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    buf.truncate(info.buffer_size());
    buf
}

fn write_png(path: &str, fb: &[u8]) {
    std::fs::create_dir_all("tests/fixtures").unwrap();
    let file = std::fs::File::create(path).unwrap();
    let mut encoder = png::Encoder::new(file, WIDTH as u32, HEIGHT as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(fb)
        .unwrap();
}

#[test]
fn dusk_parallax_uses_bg_imports_and_obj_import() {
    let (fb, e) = render_demo(&dusk_concat());
    assert!(e
        .import_reports()
        .iter()
        .any(|r| matches!(r, ImportBudget::Tile { layer: 0, .. })));
    assert!(e
        .import_reports()
        .iter()
        .any(|r| matches!(r, ImportBudget::Tile { layer: 1, .. })));
    assert!(e
        .import_reports()
        .iter()
        .any(|r| matches!(r, ImportBudget::Obj { .. })));
    assert!(e.memory().oam[0].on);
    assert!(fb
        .chunks_exact(4)
        .any(|px| px[3] == 255 && px[..3] != [0, 0, 0]));
}

#[test]
fn dusk_parallax_draws_sky_above_horizon() {
    let (fb, _) = render_demo(&dusk_concat());
    let px = &fb[(20 * WIDTH + 20) * 4..][..4];
    assert_ne!(px, &[0, 0, 0, 255], "sky pixel was backdrop black");
}

#[test]
fn dusk_parallax_draws_obj_sprite_over_hills() {
    let (fb, _) = render_demo(&dusk_concat());
    let lower_half_has_sprite_yellow = (120..155).any(|y| {
        (0..WIDTH).any(|x| {
            let p = &fb[(y * WIDTH + x) * 4..][..4];
            p[0] > 180 && p[1] > 150 && p[2] < 80 && p[3] == 255
        })
    });
    assert!(
        lower_half_has_sprite_yellow,
        "OBJ sprite was hidden by BG layers"
    );
}

#[test]
fn mode7_floor_uses_interleaved_mode7_import() {
    let (_fb, e) = render_demo(MODE7_SRC);
    assert!(e
        .import_reports()
        .iter()
        .any(|r| matches!(r, ImportBudget::Mode7 { layer: 0, .. })));
    assert!(e.memory().vram[..64].iter().any(|w| (w >> 8) != 0));
}

#[test]
fn mode7_floor_draws_below_horizon() {
    let (fb, _) = render_demo(MODE7_SRC);
    let px = &fb[(160 * WIDTH + 128) * 4..][..4];
    assert_ne!(px, &[0, 0, 0, 255], "floor pixel was backdrop black");
}

#[test]
fn dusk_parallax_demo_matches_golden_png() {
    assert!(Path::new(DUSK_GOLDEN).exists());
    let (actual, _) = render_demo(&dusk_concat());
    let expected = decode_png(DUSK_GOLDEN);
    assert_eq!(actual.len(), expected.len());
    assert!(
        actual == expected,
        "dusk demo framebuffer differs from golden PNG"
    );
}

#[test]
fn mode7_floor_demo_matches_golden_png() {
    assert!(Path::new(MODE7_GOLDEN).exists());
    let (actual, _) = render_demo(MODE7_SRC);
    let expected = decode_png(MODE7_GOLDEN);
    assert_eq!(actual.len(), expected.len());
    assert!(
        actual == expected,
        "mode7 demo framebuffer differs from golden PNG"
    );
}

#[test]
#[ignore = "regenerates the committed dusk demo golden PNG"]
fn regen_golden_dusk_parallax() {
    let (fb, _) = render_demo(&dusk_concat());
    write_png(DUSK_GOLDEN, &fb);
}

#[test]
#[ignore = "regenerates the committed Mode 7 demo golden PNG"]
fn regen_golden_mode7_floor() {
    let (fb, _) = render_demo(MODE7_SRC);
    write_png(MODE7_GOLDEN, &fb);
}

#[test]
fn offset_per_tile_demo_writes_bg3_table_and_draws() {
    let (fb, e) = render_demo(OFFSET_SRC);
    assert_eq!(e.memory().vram[0x0800] & 0x2000, 0x2000);
    assert_eq!(e.memory().vram[0x0800 + 32] & 0xa000, 0xa000);
    assert!(fb
        .chunks_exact(4)
        .any(|px| px[3] == 255 && px[..3] != [0, 0, 0]));
}

#[test]
fn offset_per_tile_demo_matches_golden_png() {
    assert!(Path::new(OFFSET_GOLDEN).exists());
    let (actual, _) = render_demo(OFFSET_SRC);
    let expected = decode_png(OFFSET_GOLDEN);
    assert_eq!(actual.len(), expected.len());
    assert_eq!(
        actual, expected,
        "offset-per-tile demo framebuffer differs from golden PNG"
    );
}

#[test]
#[ignore = "regenerates the committed offset-per-tile demo golden PNG"]
fn regen_golden_offset_per_tile() {
    let (fb, _) = render_demo(OFFSET_SRC);
    write_png(OFFSET_GOLDEN, &fb);
}

#[test]
fn mode3_gradient_demo_imports_bg1_8bpp_and_draws() {
    let (fb, e) = render_demo(MODE3_SRC);
    // 8bpp path: the gradient needs >16 colours, so it cannot be a 4bpp import.
    let colors = e.import_reports().iter().find_map(|r| match r {
        ImportBudget::Tile { layer: 0, report } => Some(report.colors_used),
        _ => None,
    });
    assert!(colors.is_some(), "BG1 tile import missing");
    assert!(
        colors.unwrap() > 16,
        "gradient must exceed the 4bpp colour count"
    );
    assert!(fb
        .chunks_exact(4)
        .any(|px| px[3] == 255 && px[..3] != [0, 0, 0]));
}

#[test]
fn mode3_gradient_demo_matches_golden_png() {
    assert!(Path::new(MODE3_GOLDEN).exists());
    let (actual, _) = render_demo(MODE3_SRC);
    let expected = decode_png(MODE3_GOLDEN);
    assert_eq!(actual.len(), expected.len());
    assert_eq!(
        actual, expected,
        "mode3 demo framebuffer differs from golden PNG"
    );
}

#[test]
#[ignore = "regenerates the committed Mode 3 demo golden PNG"]
fn regen_golden_mode3_gradient() {
    let (fb, _) = render_demo(MODE3_SRC);
    write_png(MODE3_GOLDEN, &fb);
}

#[test]
fn mode0_bands_demo_writes_per_layer_cgram_bands_and_draws() {
    let (fb, e) = render_demo(MODE0_SRC);
    assert!(e
        .import_reports()
        .iter()
        .any(|r| matches!(r, ImportBudget::Tile { layer: 0, .. })));
    assert!(e
        .import_reports()
        .iter()
        .any(|r| matches!(r, ImportBudget::Tile { layer: 1, .. })));
    let cg = &e.memory().cgram;
    assert_ne!(cg[1], 0, "BG1 colour missing from band 0");
    assert_ne!(cg[33], 0, "BG2 colour missing from band 1 (offset 32)");
    assert_ne!(cg[1], cg[33], "layers must occupy distinct CGRAM bands");
    assert!(fb
        .chunks_exact(4)
        .any(|px| px[3] == 255 && px[..3] != [0, 0, 0]));
}

#[test]
fn mode0_bands_demo_matches_golden_png() {
    assert!(Path::new(MODE0_GOLDEN).exists());
    let (actual, _) = render_demo(MODE0_SRC);
    let expected = decode_png(MODE0_GOLDEN);
    assert_eq!(actual.len(), expected.len());
    assert_eq!(
        actual, expected,
        "mode0 demo framebuffer differs from golden PNG"
    );
}

#[test]
#[ignore = "regenerates the committed Mode 0 demo golden PNG"]
fn regen_golden_mode0_bands() {
    let (fb, _) = render_demo(MODE0_SRC);
    write_png(MODE0_GOLDEN, &fb);
}

#[test]
fn translucency_demo_blends_panel_half_over_scene() {
    let (fb, _) = render_demo(TRANSLUCENCY_SRC);
    // A column inside the panel band (y=120) blends panel+scene at half; a column
    // in the same x but below the panel (y=200) shows the scene alone.
    let panel_px = &fb[(120 * WIDTH + 128) * 4..][..4];
    let scene_px = &fb[(200 * WIDTH + 128) * 4..][..4];
    assert_ne!(panel_px[..3], [0, 0, 0], "glass pixel went black");
    assert_ne!(scene_px[..3], [0, 0, 0], "scene pixel went black");
    // Half-blend pulls the bright cyan panel toward the darker scene: the blended
    // green channel is below the panel's own ~230 full value.
    assert!(
        panel_px[1] < 230,
        "no half-blend darkening applied to the panel"
    );
}

#[test]
fn translucency_demo_matches_golden_png() {
    assert!(Path::new(TRANSLUCENCY_GOLDEN).exists());
    let (actual, _) = render_demo(TRANSLUCENCY_SRC);
    let expected = decode_png(TRANSLUCENCY_GOLDEN);
    assert_eq!(actual.len(), expected.len());
    assert_eq!(
        actual, expected,
        "translucency demo differs from golden PNG"
    );
}

#[test]
#[ignore = "regenerates the committed translucency demo golden PNG"]
fn regen_golden_translucency() {
    let (fb, _) = render_demo(TRANSLUCENCY_SRC);
    write_png(TRANSLUCENCY_GOLDEN, &fb);
}

#[test]
fn spotlight_demo_masks_scene_to_a_circular_iris() {
    let (fb, _) = render_demo(SPOTLIGHT_SRC);
    // Center of the iris shows the scene; a far corner (well outside r=70) is clipped black.
    let center = &fb[(112 * WIDTH + 128) * 4..][..4];
    let corner = &fb[(5 * WIDTH + 5) * 4..][..4];
    assert_ne!(center[..3], [0, 0, 0], "iris centre was clipped");
    assert_eq!(corner[..3], [0, 0, 0], "outside the iris should be black");
}

#[test]
fn spotlight_demo_matches_golden_png() {
    assert!(Path::new(SPOTLIGHT_GOLDEN).exists());
    let (actual, _) = render_demo(SPOTLIGHT_SRC);
    let expected = decode_png(SPOTLIGHT_GOLDEN);
    assert_eq!(actual.len(), expected.len());
    assert_eq!(actual, expected, "spotlight demo differs from golden PNG");
}

#[test]
#[ignore = "regenerates the committed spotlight demo golden PNG"]
fn regen_golden_spotlight() {
    let (fb, _) = render_demo(SPOTLIGHT_SRC);
    write_png(SPOTLIGHT_GOLDEN, &fb);
}

#[test]
fn glow_demo_adds_fixed_color_over_baseline() {
    let (glow, _) = render_demo(GLOW_SRC);
    // Baseline: identical scene with no colour math.
    let baseline_src = GLOW_SRC
        .replace("color.on.bg1 = true", "color.on.bg1 = false")
        .replace("color.fixed = rgb(120, 60, 0)", "color.fixed = 0");
    let (base, _) = render_demo(&baseline_src);
    // The additive red channel must lift the frame overall (sum of R over the frame).
    let sum_r = |fb: &[u8]| fb.chunks_exact(4).map(|p| p[0] as u64).sum::<u64>();
    assert!(
        sum_r(&glow) > sum_r(&base),
        "additive glow did not brighten the frame"
    );
}

#[test]
fn glow_demo_matches_golden_png() {
    assert!(Path::new(GLOW_GOLDEN).exists());
    let (actual, _) = render_demo(GLOW_SRC);
    let expected = decode_png(GLOW_GOLDEN);
    assert_eq!(actual.len(), expected.len());
    assert_eq!(actual, expected, "glow demo differs from golden PNG");
}

#[test]
#[ignore = "regenerates the committed additive-glow demo golden PNG"]
fn regen_golden_glow() {
    let (fb, _) = render_demo(GLOW_SRC);
    write_png(GLOW_GOLDEN, &fb);
}

#[test]
fn tm_mask_demo_removes_bg2_from_main_screen() {
    let (fb, _) = render_demo(TM_MASK_SRC);
    // mode0_bg2 is magenta (R high, B high, G low). With BG2 masked off TM, no
    // pixel in the frame should read as that magenta.
    let has_magenta = fb
        .chunks_exact(4)
        .any(|p| p[0] > 150 && p[2] > 120 && p[1] < 100 && p[3] == 255);
    assert!(!has_magenta, "BG2 magenta leaked despite TM=0x01");
    // BG1 green is still present.
    let has_green = fb
        .chunks_exact(4)
        .any(|p| p[1] > 150 && p[0] < 100 && p[3] == 255);
    assert!(has_green, "BG1 green missing");
}

#[test]
fn shadow_demo_subtracts_fixed_color_below_baseline() {
    let (shadow, _) = render_demo(SHADOW_SRC);
    let baseline_src = SHADOW_SRC
        .replace("CGADSUB = 0x81", "CGADSUB = 0x00")
        .replace("COLDATA = rgb(120, 120, 120)", "COLDATA = 0");
    let (base, _) = render_demo(&baseline_src);
    let sum = |fb: &[u8]| {
        fb.chunks_exact(4)
            .map(|p| p[0] as u64 + p[1] as u64 + p[2] as u64)
            .sum::<u64>()
    };
    assert!(
        sum(&shadow) < sum(&base),
        "subtract did not darken the frame"
    );
}

#[test]
fn tm_mask_demo_matches_golden_png() {
    assert!(Path::new(TM_MASK_GOLDEN).exists());
    let (actual, _) = render_demo(TM_MASK_SRC);
    let expected = decode_png(TM_MASK_GOLDEN);
    assert_eq!(actual, expected, "tm-mask demo differs from golden PNG");
}

#[test]
#[ignore = "regenerates the committed TM-mask golden PNG"]
fn regen_golden_tm_mask() {
    let (fb, _) = render_demo(TM_MASK_SRC);
    write_png(TM_MASK_GOLDEN, &fb);
}

#[test]
fn shadow_demo_matches_golden_png() {
    assert!(Path::new(SHADOW_GOLDEN).exists());
    let (actual, _) = render_demo(SHADOW_SRC);
    let expected = decode_png(SHADOW_GOLDEN);
    assert_eq!(actual, expected, "shadow demo differs from golden PNG");
}

#[test]
#[ignore = "regenerates the committed subtractive-shadow golden PNG"]
fn regen_golden_shadow() {
    let (fb, _) = render_demo(SHADOW_SRC);
    write_png(SHADOW_GOLDEN, &fb);
}

#[test]
fn sprite_storm_overflows_both_caps_and_flickers() {
    // Both per-line caps engage on the packed band.
    let (fb, ov) = render_storm(90);
    assert!(
        ov.range_over,
        "sprite-storm must exceed the 32-sprite range cap"
    );
    assert!(
        ov.time_over,
        "sprite-storm must exceed the 34-tile time cap"
    );
    assert!(ov.max_sprites > 32);
    // Sprites actually draw over the backdrop.
    assert!(fb
        .chunks_exact(4)
        .any(|p| p[3] == 255 && p[..3] != [0, 0, 0]));
    // Authentic flicker: rotating the OAM start each frame changes the output.
    assert!(
        render_storm(90).0 != render_storm(91).0,
        "OAM rotation must change survivors"
    );
}

#[test]
fn sprite_storm_demo_matches_golden_png() {
    assert!(Path::new(SPRITE_STORM_GOLDEN).exists());
    let mut e = demo_engine(SPRITE_STORM_SRC);
    let lt = e.frame(1.0, 90).unwrap();
    let actual = render_frame(&lt, e.memory());
    let expected = decode_png(SPRITE_STORM_GOLDEN);
    assert_eq!(actual.len(), expected.len());
    assert_eq!(
        actual, expected,
        "sprite-storm demo differs from golden PNG"
    );
}

#[test]
#[ignore = "regenerates the committed sprite-storm demo golden PNG"]
fn regen_golden_sprite_storm() {
    let mut e = demo_engine(SPRITE_STORM_SRC);
    let lt = e.frame(1.0, 90).unwrap();
    write_png(SPRITE_STORM_GOLDEN, &render_frame(&lt, e.memory()));
}

// ── M8 effects demos: mosaic / Mode 7 EXTBG / direct colour ──────────────────

#[test]
fn mosaic_demo_pixelates_bg1_into_8px_blocks() {
    let (fb, _) = render_demo(MOSAIC_SRC);
    // f=60 -> mosaic size 7 -> 8px blocks; each block replicates its top-left texel.
    for &(x, y) in &[(1usize, 0usize), (7, 0), (0, 7), (7, 7)] {
        assert_eq!(
            px(&fb, x, y),
            px(&fb, 0, 0),
            "block(0,0) not flat at ({x},{y})"
        );
    }
    // adjacent block differs (ramp steps within 8px); period-32 block matches.
    assert_ne!(px(&fb, 8, 0), px(&fb, 0, 0), "block(8,0) should differ");
    assert_eq!(
        px(&fb, 32, 0),
        px(&fb, 0, 0),
        "ramp period 32 aligns with blocks"
    );
    // vs mosaic OFF: the fine sub-block detail survives -> the frame differs.
    let off = MOSAIC_SRC.replace("bg[1].mosaic = true", "bg[1].mosaic = false");
    let (base, _) = render_demo(&off);
    assert_ne!(base, fb, "mosaic did not change the frame");
}

#[test]
fn mosaic_demo_matches_golden_png() {
    assert!(Path::new(MOSAIC_GOLDEN).exists());
    let (actual, _) = render_demo(MOSAIC_SRC);
    let expected = decode_png(MOSAIC_GOLDEN);
    assert_eq!(actual.len(), expected.len());
    assert_eq!(actual, expected, "mosaic demo differs from golden PNG");
}

#[test]
#[ignore = "regenerates the committed mosaic demo golden PNG"]
fn regen_golden_mosaic() {
    let (fb, _) = render_demo(MOSAIC_SRC);
    write_png(MOSAIC_GOLDEN, &fb);
}

fn is_red(p: &[u8]) -> bool {
    p[0] > 150 && p[1] < 120 && p[2] < 120 && p[3] == 255
}
fn is_yellow(p: &[u8]) -> bool {
    p[0] > 180 && p[1] > 180 && p[2] < 100 && p[3] == 255
}

#[test]
fn extbg_demo_places_sprite_between_floor_priority_levels() {
    let (fb, _) = render_demo(EXTBG_SRC);
    // Sprite spans x 112..144, y 88..120. Left of the x=128 split -> HIGH floor covers it;
    // right of the split -> LOW floor, the sprite shows through.
    assert!(
        is_red(px(&fb, 120, 104)),
        "high floor must cover the sprite left of split"
    );
    assert!(
        is_yellow(px(&fb, 136, 104)),
        "sprite must ride over the low floor right of split"
    );
    // Floor away from the sprite is red on both halves (same colour, different priority).
    assert!(is_red(px(&fb, 40, 104)), "left floor red");
    assert!(is_red(px(&fb, 210, 104)), "right floor red");
    // EXTBG off -> the sprite flat-overlays BOTH halves, so the left pixel is yellow.
    let off = EXTBG_SRC.replace("m7.extbg = true", "m7.extbg = false");
    let (flat, _) = render_demo(&off);
    assert!(
        is_yellow(px(&flat, 120, 104)),
        "EXTBG off should overlay the sprite everywhere"
    );
}

#[test]
fn extbg_demo_matches_golden_png() {
    assert!(Path::new(EXTBG_GOLDEN).exists());
    let (actual, _) = render_demo(EXTBG_SRC);
    let expected = decode_png(EXTBG_GOLDEN);
    assert_eq!(actual.len(), expected.len());
    assert_eq!(actual, expected, "extbg demo differs from golden PNG");
}

#[test]
#[ignore = "regenerates the committed Mode 7 EXTBG demo golden PNG"]
fn regen_golden_extbg() {
    let (fb, _) = render_demo(EXTBG_SRC);
    write_png(EXTBG_GOLDEN, &fb);
}

#[test]
fn direct_color_demo_bypasses_empty_cgram_into_smooth_gradient() {
    let (fb, e) = render_demo(DIRECT_SRC);
    // CGRAM is untouched (all zero) yet the floor is fully coloured -> direct-colour bypass.
    assert!(
        e.memory().cgram.iter().all(|&c| c == 0),
        "CGRAM must stay empty"
    );
    // Every pixel opaque (idx >= 64, never 0) -> full-screen gradient, no backdrop.
    assert!(
        fb.chunks_exact(4).all(|p| p[3] == 255),
        "gradient must fill the frame"
    );
    // Many distinct colours despite an empty palette.
    let colors = fb
        .chunks_exact(4)
        .map(|p| (p[0], p[1], p[2]))
        .collect::<std::collections::HashSet<_>>();
    assert!(
        colors.len() > 32,
        "expected a rich gradient, got {} colours",
        colors.len()
    );
    // Smooth axes: red rises left->right, green rises top->bottom.
    assert!(
        px(&fb, 248, 0)[0] > px(&fb, 0, 0)[0],
        "red should rise with x"
    );
    assert!(
        px(&fb, 0, 216)[1] > px(&fb, 0, 0)[1],
        "green should rise with y"
    );
}

#[test]
fn direct_color_demo_matches_golden_png() {
    assert!(Path::new(DIRECT_GOLDEN).exists());
    let (actual, _) = render_demo(DIRECT_SRC);
    let expected = decode_png(DIRECT_GOLDEN);
    assert_eq!(actual.len(), expected.len());
    assert_eq!(
        actual, expected,
        "direct-color demo differs from golden PNG"
    );
}

#[test]
#[ignore = "regenerates the committed direct-color demo golden PNG"]
fn regen_golden_direct_color() {
    let (fb, _) = render_demo(DIRECT_SRC);
    write_png(DIRECT_GOLDEN, &fb);
}

#[test]
fn multi_file_split_renders_identical_to_single_file() {
    let (single, _) = render_demo(OFFSET_SRC);

    let (helper, rest) = OFFSET_SRC.split_once("function frame").unwrap();
    let main = format!("function frame{rest}");
    let mut e = demo_engine_files(&[("util.lua", helper), ("main.lua", &main)]);
    let lt = e.frame(1.0, 60).unwrap();
    let multi = render_frame(&lt, e.memory());

    assert!(
        single == multi,
        "multi-file split must be framebuffer-identical"
    );
}

#[test]
fn dusk_parallax_multi_file_matches_golden_png() {
    let mut e = demo_engine_files(&[
        ("main.lua", DUSK_MAIN_SRC),
        ("palette.lua", DUSK_PALETTE_SRC),
    ]);
    let lt = e.frame(1.0, 60).unwrap();
    let multi = render_frame(&lt, e.memory());
    let expected = decode_png(DUSK_GOLDEN);
    assert_eq!(multi.len(), expected.len());
    assert!(
        multi == expected,
        "multi-file dusk must match the committed golden PNG"
    );
}

#[test]
fn dusk_parallax_multi_file_matches_single_file_concat() {
    let (single, _) = render_demo(&dusk_concat());
    let mut e = demo_engine_files(&[
        ("main.lua", DUSK_MAIN_SRC),
        ("palette.lua", DUSK_PALETTE_SRC),
    ]);
    let lt = e.frame(1.0, 60).unwrap();
    let multi = render_frame(&lt, e.memory());
    assert!(
        single == multi,
        "flagship split must be framebuffer-identical to its concatenation"
    );
}
