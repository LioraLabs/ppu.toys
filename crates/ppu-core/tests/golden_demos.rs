//! Flagship demo golden tests through the real Lua/importer/render pipeline.
use ppu_core::{render_frame, ImportBudget, LuaEngine, WIDTH, HEIGHT};
use std::path::Path;

const DUSK_GOLDEN: &str = "tests/fixtures/golden_dusk_parallax.png";
const MODE7_GOLDEN: &str = "tests/fixtures/golden_mode7_floor.png";

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

fn demo_engine(src: &str) -> LuaEngine {
    let mut e = LuaEngine::new();
    e.upload_asset("sky".into(), WIDTH as u32, HEIGHT as u32, sky());
    e.upload_asset("hills".into(), WIDTH as u32, HEIGHT as u32, hills());
    e.upload_asset("hero".into(), 64, 8, hero());
    e.upload_asset("track".into(), 1024, 1024, track());
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
    encoder.write_header().unwrap().write_image_data(fb).unwrap();
}

#[test]
fn dusk_parallax_uses_bg_imports_and_obj_import() {
    let (fb, e) = render_demo(DUSK_SRC);
    assert!(e.import_reports().iter().any(|r| matches!(r, ImportBudget::Tile { layer: 0, .. })));
    assert!(e.import_reports().iter().any(|r| matches!(r, ImportBudget::Tile { layer: 1, .. })));
    assert!(e.import_reports().iter().any(|r| matches!(r, ImportBudget::Obj { .. })));
    assert!(e.memory().oam[0].on);
    assert!(fb.chunks_exact(4).any(|px| px[3] == 255 && px[..3] != [0, 0, 0]));
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
    assert!(lower_half_has_sprite_yellow, "OBJ sprite was hidden by BG layers");
}

#[test]
fn mode7_floor_uses_interleaved_mode7_import() {
    let (_fb, e) = render_demo(MODE7_SRC);
    assert!(e.import_reports().iter().any(|r| matches!(r, ImportBudget::Mode7 { layer: 0, .. })));
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
    assert!(actual == expected, "dusk demo framebuffer differs from golden PNG");
}

#[test]
fn mode7_floor_demo_matches_golden_png() {
    assert!(Path::new(MODE7_GOLDEN).exists());
    let (actual, _) = render_demo(MODE7_SRC);
    let expected = decode_png(MODE7_GOLDEN);
    assert_eq!(actual.len(), expected.len());
    assert!(actual == expected, "mode7 demo framebuffer differs from golden PNG");
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
