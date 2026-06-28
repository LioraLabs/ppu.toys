//! Golden framebuffer compare for the OBJ rasterizer — a hand-authored OAM with
//! sprites at different tiles, palettes, sizes, flips and priorities. No GPU.
use ppu_core::{rgb15, render_sprites, Memory, Obj, Source, HEIGHT, WIDTH};
use std::path::Path;

const GOLDEN: &str = "tests/fixtures/golden_sprite.png";

/// Build a 2x2-tile (16x16 px) OBJ sheet of colour indices and an OAM that
/// exercises pal selection, sizes, flips and priority overlap.
fn fixture() -> Memory {
    // Sheet: tile 0 = index 1, tile 1 = index 2, tile 2 = index 3, tile 3 = a
    // left/right split (1 | 4) so flips are visible. tiles_per_row = 2.
    let (w, h) = (16u32, 16u32);
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for ty in 0..h {
        for tx in 0..w {
            let cell = (tx / 8) + (ty / 8) * 2; // 0..3
            let index: u8 = match cell {
                0 => 1,
                1 => 2,
                2 => 3,
                _ => if (tx % 8) < 4 { 1 } else { 4 }, // tile 3 split
            };
            let i = ((ty * w + tx) * 4) as usize;
            rgba[i] = index;
            rgba[i + 3] = 255;
        }
    }

    let mut mem = Memory::new();
    mem.sources.insert("sheet".into(), Source { width: w, height: h, rgba });
    mem.obj_sheet = Some("sheet".into());

    // Backdrop + OBJ palettes (cgram 128.. for pal 0, 144.. for pal 1).
    mem.cgram[0] = rgb15(8, 8, 24);
    mem.cgram[128 + 1] = rgb15(248, 0, 0); // pal0 idx1 red
    mem.cgram[128 + 2] = rgb15(0, 248, 0); // pal0 idx2 green
    mem.cgram[128 + 3] = rgb15(0, 0, 248); // pal0 idx3 blue
    mem.cgram[128 + 4] = rgb15(248, 248, 0); // pal0 idx4 yellow
    mem.cgram[144 + 1] = rgb15(248, 0, 248); // pal1 idx1 magenta
    mem.cgram[144 + 4] = rgb15(0, 248, 248); // pal1 idx4 cyan

    // A spread of sprites.
    mem.oam[0] = Obj { on: true, x: 20.0, y: 30.0, tile: 0, pal: 0, size: 0, ..Obj::default() };
    mem.oam[1] = Obj { on: true, x: 40.0, y: 30.0, tile: 1, pal: 0, size: 0, ..Obj::default() };
    mem.oam[2] = Obj { on: true, x: 60.0, y: 30.0, tile: 2, pal: 0, size: 0, ..Obj::default() };
    // 16x16 sprite (size 1) reads the whole sheet block; pal 1.
    mem.oam[3] = Obj { on: true, x: 90.0, y: 60.0, tile: 0, pal: 1, size: 1, ..Obj::default() };
    // tile-3 split, normal and flipped, to show mirroring.
    mem.oam[4] = Obj { on: true, x: 120.0, y: 30.0, tile: 3, pal: 0, size: 0, ..Obj::default() };
    mem.oam[5] = Obj { on: true, x: 130.0, y: 30.0, tile: 3, pal: 0, size: 0, flip_x: true, ..Obj::default() };
    // Priority overlap: low-prio first, high-prio second at the same spot.
    mem.oam[6] = Obj { on: true, x: 160.0, y: 30.0, tile: 0, pal: 0, prio: 0, size: 0, ..Obj::default() };
    mem.oam[7] = Obj { on: true, x: 164.0, y: 34.0, tile: 2, pal: 1, prio: 3, size: 0, ..Obj::default() };

    mem
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
fn rasterized_sprites_match_golden_png() {
    assert!(
        Path::new(GOLDEN).exists(),
        "golden missing — run: cargo test -p ppu-core --test golden_sprite regen_golden_sprite -- --ignored"
    );
    let actual = render_sprites(&fixture(), WIDTH, HEIGHT);
    let expected = decode_png(GOLDEN);
    assert_eq!(actual.len(), WIDTH * HEIGHT * 4);
    assert_eq!(actual, expected, "sprite framebuffer differs from golden PNG");
}

#[test]
#[ignore = "regenerates the committed golden PNG"]
fn regen_golden_sprite() {
    let fb = render_sprites(&fixture(), WIDTH, HEIGHT);
    std::fs::create_dir_all("tests/fixtures").unwrap();
    let file = std::fs::File::create(GOLDEN).unwrap();
    let mut encoder = png::Encoder::new(file, WIDTH as u32, HEIGHT as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header().unwrap().write_image_data(&fb).unwrap();
}
