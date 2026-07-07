//! Golden framebuffer compare for the Mode 7 affine floor over byte-interleaved
//! VRAM — the namesake transform from the project's mode7-floor example, on a
//! hand-authored map/char/CGRAM fixture (no importer; that's m4/m7-importer).
//! No GPU, no JS.
use ppu_core::{render_mode7, rgb15, LineTableBuilder, LineTableRow, Memory, HEIGHT, WIDTH};
use std::path::Path;

const GOLDEN: &str = "tests/fixtures/golden_mode7.png";

/// Poke the LOW byte of the map word for tilemap cell (tx, ty).
fn set_map(mem: &mut Memory, tx: usize, ty: usize, tile: u8) {
    let i = ty * 128 + tx;
    mem.vram[i] = (mem.vram[i] & 0xff00) | tile as u16;
}

/// Poke the HIGH byte (char lane) of tile `tile`'s pixel (fx, fy).
fn set_char(mem: &mut Memory, tile: usize, fx: usize, fy: usize, px: u8) {
    let i = tile * 64 + fy * 8 + fx;
    mem.vram[i] = (mem.vram[i] & 0x00ff) | ((px as u16) << 8);
}

/// Hand-authored interleaved-VRAM "track": tile 1 = solid red, tile 2 = solid
/// blue, tile 3 = 2px white/green checker. The 128x128 map alternates tiles
/// 1/2 like a checkerboard with a tile-3 stripe every 16th row/column, so the
/// perspective warp is legible in the golden image.
fn track() -> Memory {
    let mut mem = Memory::new();
    mem.cgram[1] = rgb15(216, 64, 64);
    mem.cgram[2] = rgb15(64, 96, 216);
    mem.cgram[3] = rgb15(240, 240, 240);
    mem.cgram[4] = rgb15(32, 160, 96);
    for fy in 0..8 {
        for fx in 0..8 {
            set_char(&mut mem, 1, fx, fy, 1);
            set_char(&mut mem, 2, fx, fy, 2);
            set_char(&mut mem, 3, fx, fy, if ((fx / 2) + (fy / 2)) % 2 == 0 { 3 } else { 4 });
        }
    }
    for ty in 0..128 {
        for tx in 0..128 {
            let tile = if tx % 16 == 0 || ty % 16 == 0 {
                3
            } else if (tx + ty) % 2 == 0 {
                1
            } else {
                2
            };
            set_map(&mut mem, tx, ty, tile);
        }
    }
    mem
}

/// The mode7-floor transform from the spec, evaluated at t = 1.0:
///   hdma(96,223, fn(y)): d = 64/(y-95); m7.a=m7.d=d; m7.cx=128,cy=0;
///                        bg[1].scroll.y = (t*80)*d
fn floor_framebuffer() -> Vec<u8> {
    let t = 1.0f32;
    let mut b = LineTableBuilder::new(LineTableRow::default());
    b.hdma(96, 223, move |y, r| {
        let d = 64.0 / (y as f32 - 95.0);
        r.m7.a = d;
        r.m7.d = d;
        r.m7.b = 0.0;
        r.m7.c = 0.0;
        r.m7.cx = 128.0;
        r.m7.cy = 0.0;
        r.bg[0].scroll_y = (t * 80.0) * d;
    });
    let lt = b.build(HEIGHT);
    render_mode7(&lt, &track(), WIDTH, HEIGHT)
}

fn decode_png(path: &str) -> Vec<u8> {
    let decoder = png::Decoder::new(std::fs::File::open(path).unwrap());
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    buf.truncate(info.buffer_size());
    buf
}

#[test]
fn mode7_floor_matches_golden_png() {
    assert!(
        Path::new(GOLDEN).exists(),
        "golden missing — run: cargo test -p ppu-core regen_mode7_golden -- --ignored"
    );
    let actual = floor_framebuffer();
    let expected = decode_png(GOLDEN);
    assert_eq!(actual.len(), WIDTH * HEIGHT * 4);
    assert_eq!(actual, expected, "Mode 7 floor framebuffer differs from golden PNG");
}

#[test]
#[ignore = "regenerates the committed golden PNG"]
fn regen_mode7_golden() {
    let fb = floor_framebuffer();
    std::fs::create_dir_all("tests/fixtures").unwrap();
    let file = std::fs::File::create(GOLDEN).unwrap();
    let mut encoder = png::Encoder::new(file, WIDTH as u32, HEIGHT as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header().unwrap().write_image_data(&fb).unwrap();
}
