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

const DUSK_SRC: &str = r#"-- ppu.toys :: dusk-parallax (Mode 1: parallax BG scroll + CGRAM colour-cycle + sprite)
local SPEED = 12
function frame(t, f)
  mode = 1; brightness = 15
  bg[1].source = "sky";   bg[2].source = "hills"
  bg[2].map_base = 0x0800; bg[2].char_base = 0x4000
  bg[1].scroll.x = t * SPEED
  bg[2].scroll.x = t * SPEED * 3
  for i = 0, 7 do cgram[0x40 + i] = hsl((t*40 + i*12) % 360, 0.6, 0.5) end
  obj[0].tile = 4; obj[0].pal = 0; obj[0].prio = 3; obj[0].x = 120; obj[0].y = 132 + sin(t*3) * 4
  obj.char_base = 0x6000; obj.sheet = "hero"; obj[0].on = true
end
"#;

const MODE7_SRC: &str = r#"-- ppu.toys :: mode7-floor (the namesake; per-scanline affine floor)
function frame(t, f)
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
  mode = 1; brightness = 15
  bg[1].source = "panel"                       -- the glass panel (main only)
  bg[2].source = "ribbons"; bg[2].char_base = 0x2000  -- scene, on main AND sub
  bg[2].map_base = 0x0800
  TM = 0x03        -- BG1 (panel) + BG2 (scene) on the main screen
  TS = 0x02        -- BG2 (scene) on the sub screen -> the addend under the glass
  CGADSUB = 0x41   -- add + half + BG1 math-enable
  CGWSEL = 0x02    -- addend = subscreen (not fixed colour)
end
"#;

const SPOTLIGHT_SRC: &str = r#"-- ppu.toys :: spotlight (per-scanline circular iris via the colour window)
function frame(t, f)
  mode = 1; brightness = 15
  bg[1].source = "ribbons"
  TM = 0x01                 -- BG1 only on the main screen
  WOBJSEL = 0x20            -- COLOR window: window-1 enable (high nibble bit1)
  WOBJLOG = 0x00            -- COLOR window logic = OR
  CGWSEL = 0x40             -- clip-to-black region = 01 (outside the window -> black)
  -- iris: per scanline, window 1 spans [cx-hw, cx+hw] where hw traces a circle.
  local cx, cy, r = 128, 112, 70
  hdma(0, 223, function(y)
    local dy = y - cy
    local inside = r*r - dy*dy
    if inside < 0 then
      WH0 = 1; WH1 = 0        -- empty span (left > right) -> nothing inside
    else
      local hw = floor(sqrt(inside))
      WH0 = cx - hw
      WH1 = cx + hw
    end
  end)
end
"#;

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

fn demo_engine(src: &str) -> LuaEngine {
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
    e.set_source(src).unwrap();
    e
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
    let (fb, e) = render_demo(DUSK_SRC);
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
    let (fb, _) = render_demo(DUSK_SRC);
    let px = &fb[(20 * WIDTH + 20) * 4..][..4];
    assert_ne!(px, &[0, 0, 0, 255], "sky pixel was backdrop black");
}

#[test]
fn dusk_parallax_draws_obj_sprite_over_hills() {
    let (fb, _) = render_demo(DUSK_SRC);
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
    let (actual, _) = render_demo(DUSK_SRC);
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
    let (fb, _) = render_demo(DUSK_SRC);
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
    assert!(panel_px[1] < 230, "no half-blend darkening applied to the panel");
}

#[test]
fn translucency_demo_matches_golden_png() {
    assert!(Path::new(TRANSLUCENCY_GOLDEN).exists());
    let (actual, _) = render_demo(TRANSLUCENCY_SRC);
    let expected = decode_png(TRANSLUCENCY_GOLDEN);
    assert_eq!(actual.len(), expected.len());
    assert_eq!(actual, expected, "translucency demo differs from golden PNG");
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
