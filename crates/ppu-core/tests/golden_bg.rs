//! Golden framebuffer compare for the Mode-1 BG rasterizer: a scrolled, wrapped,
//! paletted BG layer through CGRAM with brightness attenuation. Mirrors the
//! golden.rs pattern but with its own fixture + PNG.
use ppu_core::{
    rgb15, render_bg_scanline, unpack_rgb15, Bg, LineTableRow, Memory, Source, HEIGHT, WIDTH,
};
use std::path::Path;

const GOLDEN: &str = "tests/fixtures/golden_bg.png";

/// Memory with a 16x16 direct-RGBA source (a deterministic color pattern with
/// some alpha-0 transparent cells so the backdrop shows through).
fn fixture_mem() -> Memory {
    let mut m = Memory::new();
    m.cgram[0] = rgb15(8, 8, 16); // backdrop
    let (w, h) = (16u32, 16u32);
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let k = ((x ^ y) & 0x0f) as u8;
            if k == 0 {
                rgba.extend_from_slice(&[0, 0, 0, 0]); // transparent cell
            } else {
                rgba.extend_from_slice(&[k * 16, 0, 255 - k * 16, 255]);
            }
        }
    }
    m.sources.insert("bg".into(), Source { width: w, height: h, rgba });
    m
}

/// A scrolled BG1 layer at brightness 11 (exercises scroll/wrap + attenuation).
fn fixture_row() -> LineTableRow {
    let mut row = LineTableRow::default();
    row.brightness = 11;
    row.bg[0] = Bg { scroll_x: 9.0, scroll_y: 5.0, source: Some("bg".into()), visible: true };
    row
}

fn render_frame() -> Vec<u8> {
    let m = fixture_mem();
    let row = fixture_row();
    let mut fb = Vec::with_capacity(WIDTH * HEIGHT * 4);
    for y in 0..HEIGHT {
        for px in render_bg_scanline(&row, &m, y, WIDTH) {
            fb.extend_from_slice(&px);
        }
    }
    fb
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
fn brightness_attenuates_backdrop_correctly() {
    // Acceptance criterion, independent of the PNG: brightness 0 -> black,
    // 15 -> identity, 11 -> scaled by 11/15.
    let mut m = fixture_mem();
    m.cgram[0] = rgb15(150, 150, 150);
    let bd = unpack_rgb15(rgb15(150, 150, 150));
    let mut row = LineTableRow::default();
    row.brightness = 0;
    assert_eq!(render_bg_scanline(&row, &m, 0, 1)[0], [0, 0, 0, 255]);
    row.brightness = 15;
    assert_eq!(render_bg_scanline(&row, &m, 0, 1)[0], bd);
    row.brightness = 11;
    let expect = [
        (bd[0] as u16 * 11 / 15) as u8,
        (bd[1] as u16 * 11 / 15) as u8,
        (bd[2] as u16 * 11 / 15) as u8,
        255,
    ];
    assert_eq!(render_bg_scanline(&row, &m, 0, 1)[0], expect);
}

#[test]
fn bg_frame_matches_golden_png() {
    assert!(
        Path::new(GOLDEN).exists(),
        "golden missing — run: cargo test -p ppu-core regen_golden_bg -- --ignored"
    );
    let actual = render_frame();
    let expected = decode_png(GOLDEN);
    assert_eq!(actual.len(), WIDTH * HEIGHT * 4);
    assert_eq!(actual, expected, "BG framebuffer differs from golden PNG");
}

#[test]
#[ignore = "regenerates the committed golden BG PNG"]
fn regen_golden_bg() {
    let fb = render_frame();
    std::fs::create_dir_all("tests/fixtures").unwrap();
    let file = std::fs::File::create(GOLDEN).unwrap();
    let mut encoder = png::Encoder::new(file, WIDTH as u32, HEIGHT as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header().unwrap().write_image_data(&fb).unwrap();
}
