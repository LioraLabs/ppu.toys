//! Golden sprite framebuffer compare over hand-authored OBJ VRAM.
//!
//! No importer: hand-drawn 4bpp OBJ chars packed into authentic bitplanes at
//! the OBSEL char base, OBJ CGRAM sub-palettes (128..), and OAM entries with
//! flips / 16x16 size / priority / off-screen X — rendered through the actual
//! `render_frame` compositor seam (sprites overlay the backdrop).
use ppu_core::{
    render_frame, rgb15, unpack_rgb15, LineTableBuilder, LineTableRow, Memory, Obj, HEIGHT, WIDTH,
};
use std::path::Path;

const GOLDEN: &str = "tests/fixtures/golden_sprite.png";
const CHAR_BASE: usize = 0x2000;

/// Write a 4bpp OBJ char (16 words) at `CHAR_BASE`, tile `n`, from an 8x8 grid.
fn put_obj(mem: &mut Memory, n: usize, grid: [[u8; 8]; 8]) {
    let base = CHAR_BASE + n * 16;
    for y in 0..8 {
        let (mut p01, mut p23) = (0u16, 0u16);
        for x in 0..8 {
            let v = grid[y][x] as u16;
            let bit = 7 - x;
            p01 |= (v & 1) << bit | ((v >> 1) & 1) << (bit + 8);
            p23 |= ((v >> 2) & 1) << bit | ((v >> 3) & 1) << (bit + 8);
        }
        mem.vram[base + y] = p01;
        mem.vram[base + 8 + y] = p23;
    }
}

fn fixture() -> (ppu_core::LineTable, Memory) {
    let mut mem = Memory::new();
    mem.obsel.char_base = CHAR_BASE as u16;
    mem.obsel.size_sel = 7; // small 16x32 (non-square) / large 32x32 (square)
    mem.cgram[0] = rgb15(24, 16, 40); // backdrop

    // Solid index-1 across a run of OBJ tiles so every sprite is a clean filled
    // rectangle regardless of size (16x32 samples an 8-tile block, 32x32 a 16-tile block).
    let solid = [[1u8; 8]; 8];
    for n in 0..96usize {
        put_obj(&mut mem, n, solid);
    }

    // Eight OBJ sub-palettes, one vivid colour each at index 1 (cgram[128 + p*16 + 1]).
    let pal_colors = [
        rgb15(224, 64, 32),
        rgb15(64, 160, 255),
        rgb15(96, 224, 120),
        rgb15(255, 200, 96),
        rgb15(220, 80, 220),
        rgb15(80, 220, 220),
        rgb15(240, 240, 120),
        rgb15(200, 200, 200),
    ];
    for (p, &c) in pal_colors.iter().enumerate() {
        mem.cgram[128 + p * 16 + 1] = c;
    }

    // A band of six 16x32 non-square sprites (large=false), alternating palettes.
    for i in 0..6usize {
        mem.oam[i] = Obj {
            on: true,
            x: (12 + i * 40) as i16,
            y: 24,
            tile: 0,
            pal: (i % 8) as u8,
            prio: (i % 4) as u8,
            large: false,
            ..Obj::default()
        };
    }
    // Two 32x32 large sprites lower down.
    mem.oam[6] = Obj { on: true, x: 40, y: 120, tile: 0, pal: 2, large: true, ..Obj::default() };
    mem.oam[7] = Obj { on: true, x: 150, y: 120, tile: 0, pal: 4, large: true, ..Obj::default() };
    // A 16x32 sprite clipped off the left edge (renderer clips per-pixel).
    mem.oam[8] = Obj { on: true, x: -8, y: 80, tile: 0, pal: 1, large: false, ..Obj::default() };

    let lt = LineTableBuilder::new(LineTableRow::default()).build(HEIGHT);
    (lt, mem)
}

fn decode_png(path: &str) -> Vec<u8> {
    let decoder = png::Decoder::new(std::fs::File::open(path).unwrap());
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    buf.truncate(info.buffer_size());
    buf
}

fn px(fb: &[u8], x: usize, y: usize) -> [u8; 4] {
    let o = (y * WIDTH + x) * 4;
    [fb[o], fb[o + 1], fb[o + 2], fb[o + 3]]
}

#[test]
fn sprite_fixture_draws_size_pair_sprites_over_backdrop() {
    let (lt, mem) = fixture();
    let fb = render_frame(&lt, &mem);
    let red = unpack_rgb15(rgb15(224, 64, 32)); // pal 0 -> first 16x32 sprite
    let green = unpack_rgb15(rgb15(96, 224, 120)); // pal 2 -> a 32x32 large sprite
    // First sprite is a filled 16x32 NON-SQUARE: top-left + far corner present, and
    // exactly 16 wide (the column one past its width is NOT its colour).
    assert_eq!(px(&fb, 12, 24), red, "non-square sprite top-left");
    assert_eq!(px(&fb, 27, 55), red, "non-square sprite fills its full 16x32 extent");
    assert_ne!(px(&fb, 28, 24), red, "non-square sprite must be only 16 px wide");
    // A large 32x32 sprite fills its whole footprint.
    assert_eq!(px(&fb, 40, 120), green, "large 32x32 top-left");
    assert_eq!(px(&fb, 71, 151), green, "large sprite fills its full 32x32 extent");
    // Backdrop where nothing is drawn.
    assert_eq!(px(&fb, 250, 210), unpack_rgb15(rgb15(24, 16, 40)), "backdrop missing");
}

#[test]
fn sprite_fixture_matches_golden_png() {
    assert!(
        Path::new(GOLDEN).exists(),
        "golden missing — run: cargo test -p ppu-core regen_golden_sprite -- --ignored"
    );
    let (lt, mem) = fixture();
    let actual = render_frame(&lt, &mem);
    let expected = decode_png(GOLDEN);
    assert_eq!(actual.len(), WIDTH * HEIGHT * 4);
    assert_eq!(actual, expected, "framebuffer differs from golden PNG");
}

#[test]
#[ignore = "regenerates the committed golden PNG"]
fn regen_golden_sprite() {
    let (lt, mem) = fixture();
    let fb = render_frame(&lt, &mem);
    std::fs::create_dir_all("tests/fixtures").unwrap();
    let file = std::fs::File::create(GOLDEN).unwrap();
    let mut encoder = png::Encoder::new(file, WIDTH as u32, HEIGHT as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&fb)
        .unwrap();
}
