//! Golden framebuffer compare — the engine test pattern (no GPU, no JS).
use ppu_core::{rasterize, LineTable, LineTableBuilder, LineTableRow, HEIGHT, WIDTH};
use std::path::Path;

const GOLDEN: &str = "tests/fixtures/golden_basic.png";

/// Hand-authored fixture: three horizontal bands, each a distinct resolved
/// register state, exercising defaults -> per-line override resolution.
fn fixture() -> LineTable {
    let mut b = LineTableBuilder::new(LineTableRow::default());
    b.hdma(0, 73, |_, r| {
        r.mode = 1;
        r.brightness = 4;
        r.bg[0].scroll_x = 10.0;
    });
    b.hdma(74, 148, |_, r| {
        r.mode = 2;
        r.brightness = 8;
        r.bg[0].scroll_x = 20.0;
    });
    b.hdma(149, 223, |_, r| {
        r.mode = 7;
        r.brightness = 15;
        r.bg[0].scroll_x = 30.0;
    });
    b.build(HEIGHT)
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
