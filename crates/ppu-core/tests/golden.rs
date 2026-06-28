//! Golden framebuffer compare — the engine test pattern (no GPU, no JS).
use ppu_core::{rasterize, LineTable, HEIGHT, WIDTH};
use std::path::Path;

const GOLDEN: &str = "tests/fixtures/golden_basic.png";

/// Hand-authored fixture: three horizontal color bands over the full frame.
fn fixture() -> LineTable {
    let rows = (0..HEIGHT)
        .map(|y| match y {
            0..=73 => [200, 40, 40, 255],
            74..=148 => [40, 200, 40, 255],
            _ => [40, 40, 200, 255],
        })
        .collect();
    LineTable { rows }
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
fn rasterized_fixture_matches_golden_png() {
    assert!(
        Path::new(GOLDEN).exists(),
        "golden missing — run: cargo test -p ppu-core regen_golden -- --ignored"
    );
    let actual = rasterize(&fixture(), WIDTH, HEIGHT);
    let expected = decode_png(GOLDEN);
    assert_eq!(actual.len(), WIDTH * HEIGHT * 4);
    assert_eq!(actual, expected, "framebuffer differs from golden PNG");
}

#[test]
#[ignore = "regenerates the committed golden PNG"]
fn regen_golden() {
    let fb = rasterize(&fixture(), WIDTH, HEIGHT);
    std::fs::create_dir_all("tests/fixtures").unwrap();
    let file = std::fs::File::create(GOLDEN).unwrap();
    let mut encoder = png::Encoder::new(file, WIDTH as u32, HEIGHT as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header().unwrap().write_image_data(&fb).unwrap();
}
