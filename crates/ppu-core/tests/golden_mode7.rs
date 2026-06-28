//! Golden framebuffer compare for the Mode 7 affine floor — the namesake
//! transform from the project's mode7-floor example. No GPU, no JS.
use ppu_core::{render_mode7, LineTableBuilder, LineTableRow, Source, HEIGHT, WIDTH};
use std::path::Path;

const GOLDEN: &str = "tests/fixtures/golden_mode7.png";

/// 64x64 procedural "track": an 8x8 grid of distinctly-colored cells so the
/// perspective warp is legible in the golden image.
fn track() -> Source {
    let (w, h) = (64u32, 64u32);
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let cx = (x / 8) as u8;
            let cy = (y / 8) as u8;
            let i = ((y * w + x) * 4) as usize;
            rgba[i] = cx * 32;
            rgba[i + 1] = cy * 32;
            rgba[i + 2] = ((cx + cy) & 1) * 255;
            rgba[i + 3] = 255;
        }
    }
    Source { width: w, height: h, rgba }
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
