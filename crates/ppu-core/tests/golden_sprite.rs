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

/// A right-pointing arrow, asymmetric on both axes so flips are visible.
const ARROW: [[u8; 8]; 8] = [
    [0, 1, 0, 0, 0, 0, 0, 0],
    [0, 1, 1, 0, 0, 0, 0, 0],
    [1, 2, 2, 3, 0, 0, 0, 0],
    [1, 2, 2, 2, 3, 3, 0, 0],
    [1, 1, 2, 3, 0, 0, 0, 0],
    [0, 1, 1, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0],
];

/// Solid 8x8 block of index `c` with a transparent 2x2 notch (orientation cue).
fn block(c: u8) -> [[u8; 8]; 8] {
    let mut g = [[c; 8]; 8];
    (g[0][0], g[0][1], g[1][0], g[1][1]) = (0, 0, 0, 0);
    g
}

fn fixture() -> (ppu_core::LineTable, Memory) {
    let mut mem = Memory::new();
    mem.obsel.char_base = CHAR_BASE as u16;
    mem.cgram[0] = rgb15(24, 16, 40); // backdrop

    // OBJ sub-palette 0 (cgram[128 + 1..]).
    let pal0 = [
        rgb15(224, 64, 32),
        rgb15(255, 200, 96),
        rgb15(255, 255, 255),
    ];
    for (i, &c) in pal0.iter().enumerate() {
        mem.cgram[128 + 1 + i] = c;
    }
    // OBJ sub-palette 2 (cgram[128 + 32 + 1..]) — a recolor for a second sprite.
    mem.cgram[128 + 32 + 1] = rgb15(64, 160, 255);
    mem.cgram[128 + 32 + 2] = rgb15(160, 224, 255);
    // OBJ sub-palette 1 for the 16x16 blocks.
    mem.cgram[128 + 16 + 4] = rgb15(96, 224, 120);
    mem.cgram[128 + 16 + 5] = rgb15(48, 160, 80);
    mem.cgram[128 + 16 + 6] = rgb15(200, 255, 210);
    mem.cgram[128 + 16 + 7] = rgb15(24, 96, 48);

    // Chars: arrow = tile 1; a 16x16 sprite = quadrant tiles 2/3/18/19.
    put_obj(&mut mem, 1, ARROW);
    put_obj(&mut mem, 2, block(4));
    put_obj(&mut mem, 3, block(5));
    put_obj(&mut mem, 18, block(6));
    put_obj(&mut mem, 19, block(7));

    // OAM: a row of arrows with alternating flips + palettes; one 16x16 block;
    // a priority-tagged arrow; an arrow clipped off the left edge.
    for i in 0..6usize {
        mem.oam[i] = Obj {
            on: true,
            x: (16 + i * 24) as i16,
            y: (20 + (i % 3) * 8) as u8,
            tile: 1,
            pal: if i % 2 == 0 { 0 } else { 2 },
            flip_x: i % 2 == 1,
            flip_y: i % 4 == 3,
            large: false,
            prio: (i % 4) as u8,
            ..Obj::default()
        };
    }
    mem.oam[6] = Obj {
        on: true,
        x: 180,
        y: 120,
        tile: 2,
        pal: 1,
        large: true,
        ..Obj::default()
    };
    mem.oam[7] = Obj {
        on: true,
        x: -4,
        y: 80,
        tile: 1,
        pal: 0,
        ..Obj::default()
    }; // clipped left

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

/// Structural sanity independent of the committed PNG: sprites and backdrop
/// both contribute pixels (guards against an all-backdrop golden being frozen).
#[test]
fn sprite_fixture_draws_sprites_over_backdrop() {
    let (lt, mem) = fixture();
    let fb = render_frame(&lt, &mem);
    let count = |c: [u8; 4]| fb.chunks(4).filter(|p| *p == c).count();
    assert!(
        count(unpack_rgb15(rgb15(224, 64, 32))) > 0,
        "arrow body missing"
    );
    assert!(
        count(unpack_rgb15(rgb15(96, 224, 120))) > 0,
        "16x16 block missing"
    );
    assert!(
        count(unpack_rgb15(rgb15(64, 160, 255))) > 0,
        "recolored arrow missing"
    );
    assert!(
        count(unpack_rgb15(rgb15(24, 16, 40))) > 0,
        "backdrop missing"
    );
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
