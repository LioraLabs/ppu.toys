//! Golden framebuffer compare for the OBJ rasterizer — a hand-authored OAM with
//! sprites at different tiles, palettes, sizes, flips and priorities. No GPU.
use ppu_core::{rgb15, render_sprites, Memory, Obj, Source, HEIGHT, WIDTH};
use std::path::Path;

const GOLDEN: &str = "tests/fixtures/golden_sprite.png";

fn fixture() -> Memory {
    // 2x2-tile (16x16) sheet, each 8x8 cell a distinct direct color; tile 3 is a
    // left/right split so flips are visible. tiles_per_row = 2.
    let (w, h) = (16u32, 16u32);
    let red = [248, 0, 0, 255];
    let green = [0, 248, 0, 255];
    let blue = [0, 0, 248, 255];
    let yellow = [248, 248, 0, 255];
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for ty in 0..h {
        for tx in 0..w {
            let cell = (tx / 8) + (ty / 8) * 2; // 0..3
            let color = match cell {
                0 => red,
                1 => green,
                2 => blue,
                _ => if (tx % 8) < 4 { red } else { yellow }, // tile 3 split
            };
            let i = ((ty * w + tx) * 4) as usize;
            rgba[i..i + 4].copy_from_slice(&color);
        }
    }

    let mut mem = Memory::new();
    mem.sources.insert("sheet".into(), Source { width: w, height: h, rgba });
    mem.obj_sheet = Some("sheet".into());
    mem.cgram[0] = rgb15(8, 8, 24); // backdrop only

    // A spread of sprites (pal is a no-op in v1; kept for surface parity).
    mem.oam[0] = Obj { on: true, x: 20, y: 30, tile: 0, pal: 0, size: 0, ..Obj::default() };
    mem.oam[1] = Obj { on: true, x: 40, y: 30, tile: 1, pal: 0, size: 0, ..Obj::default() };
    mem.oam[2] = Obj { on: true, x: 60, y: 30, tile: 2, pal: 0, size: 0, ..Obj::default() };
    mem.oam[3] = Obj { on: true, x: 90, y: 60, tile: 0, pal: 1, size: 1, ..Obj::default() };
    mem.oam[4] = Obj { on: true, x: 120, y: 30, tile: 3, pal: 0, size: 0, ..Obj::default() };
    mem.oam[5] = Obj { on: true, x: 130, y: 30, tile: 3, pal: 0, size: 0, flip_x: true, ..Obj::default() };
    mem.oam[6] = Obj { on: true, x: 160, y: 30, tile: 0, pal: 0, prio: 0, size: 0, ..Obj::default() };
    mem.oam[7] = Obj { on: true, x: 164, y: 34, tile: 2, pal: 1, prio: 3, size: 0, ..Obj::default() };

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
