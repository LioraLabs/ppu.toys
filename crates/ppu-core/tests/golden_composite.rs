//! Golden framebuffer compare for the E5 compositor: a Mode-1 HUD band over a
//! Mode-7 floor band (the split-screen unlock), plus a sprite, exercising
//! per-line mode switch + backdrop + BG + sprite + brightness in one frame.
use ppu_core::{
    render_frame, rgb15, Bg, LineTableBuilder, LineTableRow, Memory, Mode7, Obj, Source, HEIGHT,
    WIDTH,
};
use std::path::Path;

const GOLDEN: &str = "tests/fixtures/golden_composite.png";

/// 16x16 direct-RGBA "hud" BG image (a simple two-tone checker with some
/// transparent cells so the backdrop shows through in the top band).
fn hud_source() -> Source {
    let (w, h) = (16u32, 16u32);
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            if (x / 4 + y / 4) % 3 == 0 {
                rgba.extend_from_slice(&[0, 0, 0, 0]); // transparent
            } else if (x ^ y) & 1 == 0 {
                rgba.extend_from_slice(&[230, 40, 40, 255]);
            } else {
                rgba.extend_from_slice(&[40, 40, 230, 255]);
            }
        }
    }
    Source { width: w, height: h, rgba }
}

/// 64x64 procedural "track" floor (8x8 colored grid), same idea as golden_mode7.
fn track_source() -> Source {
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

/// 8x8 solid opaque sprite sheet (one tile).
fn sprite_sheet() -> Source {
    let mut rgba = vec![0u8; 8 * 8 * 4];
    for px in rgba.chunks_mut(4) {
        px.copy_from_slice(&[255, 240, 0, 255]);
    }
    Source { width: 8, height: 8, rgba }
}

fn fixture_mem() -> Memory {
    let mut m = Memory::new();
    m.cgram[0] = rgb15(12, 12, 28); // backdrop
    m.sources.insert("hud".into(), hud_source());
    m.sources.insert("track".into(), track_source());
    m.sources.insert("sheet".into(), sprite_sheet());
    m.obj_sheet = Some("sheet".into());
    // A HUD sprite living in the top (Mode 1) band.
    m.oam[0] = Obj { on: true, x: 120, y: 40, tile: 0, size: 1, ..Obj::default() };
    m
}

/// Top band (rows 0..111): Mode 1 HUD over the hud image, brightness 15.
/// Bottom band (rows 112..223): Mode 7 floor over the track, perspective warp.
fn fixture_linetable() -> ppu_core::LineTable {
    let t = 1.0f32;
    let mut def = LineTableRow::default();
    def.mode = 1;
    def.brightness = 15;
    def.bg[0] = Bg { scroll_x: 4.0, scroll_y: 2.0, source: Some("hud".into()), visible: true };
    let mut b = LineTableBuilder::new(def);
    b.hdma(112, 223, move |y, r| {
        let d = 64.0 / (y as f32 - 111.0); // receding floor from the split line
        r.mode = 7;
        r.brightness = 15;
        r.bg[0] = Bg { scroll_x: 0.0, scroll_y: (t * 80.0) * d, source: Some("track".into()), visible: true };
        r.m7 = Mode7 { a: d, b: 0.0, c: 0.0, d, cx: 128.0, cy: 0.0 };
    });
    b.build(HEIGHT)
}

fn frame() -> Vec<u8> {
    render_frame(&fixture_linetable(), &fixture_mem())
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
fn per_line_mode_switch_changes_bands() {
    // Acceptance, independent of the PNG: the split line actually switches mode.
    let lt = fixture_linetable();
    assert_eq!(lt.rows[10].mode, 1); // top band = Mode 1
    assert_eq!(lt.rows[200].mode, 7); // bottom band = Mode 7
}

#[test]
fn composite_frame_matches_golden_png() {
    assert!(
        Path::new(GOLDEN).exists(),
        "golden missing — run: cargo test -p ppu-core --test golden_composite regen_golden_composite -- --ignored"
    );
    let actual = frame();
    let expected = decode_png(GOLDEN);
    assert_eq!(actual.len(), WIDTH * HEIGHT * 4);
    assert_eq!(actual, expected, "composite framebuffer differs from golden PNG");
}

#[test]
#[ignore = "regenerates the committed golden composite PNG"]
fn regen_golden_composite() {
    let fb = frame();
    std::fs::create_dir_all("tests/fixtures").unwrap();
    let file = std::fs::File::create(GOLDEN).unwrap();
    let mut encoder = png::Encoder::new(file, WIDTH as u32, HEIGHT as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header().unwrap().write_image_data(&fb).unwrap();
}
