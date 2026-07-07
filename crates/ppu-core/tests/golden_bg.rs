//! Golden BG framebuffer compare over hand-authored VRAM (Mode 1).
//!
//! No importer involved: hand-drawn index grids packed into authentic
//! bitplanes, hand-placed `vhopppcc cccccccc` tilemap entries, CGRAM
//! sub-palettes, and the real binding registers, rendered through the actual
//! compositor seam. Exercises 4bpp (BG1/BG2) + 2bpp (BG3), H/V flip, 16x16
//! quadrant tiles, 64x32 screen wrap, negative scroll, sub-palette
//! indirection, and an HDMA scroll band.
use ppu_core::{
    render_frame, rgb15, unpack_rgb15, LineTable, LineTableBuilder, LineTableRow, Memory,
    HEIGHT, WIDTH,
};
use std::path::Path;

const GOLDEN: &str = "tests/fixtures/golden_bg.png";

/// Pack a hand-drawn 8x8 index grid into 2bpp bitplanes (8 words; word `y` =
/// plane 0 in the low byte, plane 1 high, bit 7 = leftmost pixel).
fn pack_2bpp(px: [[u8; 8]; 8]) -> [u16; 8] {
    std::array::from_fn(|y| {
        (0..8).fold(0u16, |w, x| {
            let bit = 7 - x;
            w | ((px[y][x] & 1) as u16) << bit | (((px[y][x] >> 1) & 1) as u16) << (bit + 8)
        })
    })
}

/// Pack a hand-drawn 8x8 index grid into 4bpp bitplanes (16 words: planes
/// 0/1 then planes 2/3).
fn pack_4bpp(px: [[u8; 8]; 8]) -> [u16; 16] {
    let p01 = pack_2bpp(px.map(|r| r.map(|v| v & 3)));
    let p23 = pack_2bpp(px.map(|r| r.map(|v| (v >> 2) & 3)));
    std::array::from_fn(|i| if i < 8 { p01[i] } else { p23[i - 8] })
}

/// Build a `vhopppcc cccccccc` tilemap entry word.
fn entry(tile: u16, pal: u16, prio: bool, fh: bool, fv: bool) -> u16 {
    tile | pal << 10 | (prio as u16) << 13 | (fh as u16) << 14 | (fv as u16) << 15
}

/// Write 4bpp char `char_i` (16 words) at the BG1/BG2 char base 0x1000.
fn put4(mem: &mut Memory, char_i: usize, grid: [[u8; 8]; 8]) {
    for (i, w) in pack_4bpp(grid).into_iter().enumerate() {
        mem.vram[0x1000 + char_i * 16 + i] = w;
    }
}

/// A right-pointing arrow, asymmetric on both axes (flips are visible).
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

/// A hollow ring with inner corner dots.
const RING: [[u8; 8]; 8] = [
    [0, 4, 4, 4, 4, 4, 4, 0],
    [4, 5, 0, 0, 0, 0, 5, 4],
    [4, 0, 0, 0, 0, 0, 0, 4],
    [4, 0, 0, 0, 0, 0, 0, 4],
    [4, 0, 0, 0, 0, 0, 0, 4],
    [4, 0, 0, 0, 0, 0, 0, 4],
    [4, 5, 0, 0, 0, 0, 5, 4],
    [0, 4, 4, 4, 4, 4, 4, 0],
];

/// 2bpp stripes-and-dots glyph for BG3.
const STRIPE: [[u8; 8]; 8] = [
    [1, 1, 1, 1, 1, 1, 1, 1],
    [0, 0, 0, 0, 0, 0, 0, 0],
    [2, 2, 2, 2, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0],
    [3, 0, 3, 0, 3, 0, 3, 0],
    [0, 0, 0, 0, 0, 0, 0, 0],
    [2, 2, 2, 2, 2, 2, 2, 2],
    [0, 0, 0, 0, 0, 0, 0, 0],
];

/// Solid 8x8 block of index `c` with a transparent 2x2 notch at its top-left,
/// so each quadrant of the 16x16 BG2 tile shows color AND orientation.
fn block(c: u8) -> [[u8; 8]; 8] {
    let mut g = [[c; 8]; 8];
    (g[0][0], g[0][1], g[1][0], g[1][1]) = (0, 0, 0, 0);
    g
}

fn fixture() -> (LineTable, Memory) {
    let mut mem = Memory::new();

    // ── CGRAM ────────────────────────────────────────────────────────────
    mem.cgram[0] = rgb15(16, 24, 48); // backdrop: deep blue
    // BG1/BG2 4bpp sub-palette 1 (cgram[16..]): indices 1..=9.
    let pal1 = [
        rgb15(224, 64, 32),
        rgb15(255, 160, 48),
        rgb15(255, 240, 96),
        rgb15(64, 200, 96),
        rgb15(96, 224, 255),
        rgb15(40, 120, 216),
        rgb15(168, 88, 232),
        rgb15(255, 128, 200),
        rgb15(240, 240, 240),
    ];
    for (i, &c) in pal1.iter().enumerate() {
        mem.cgram[16 + 1 + i] = c;
    }
    // BG1 4bpp sub-palette 2: a cool recolor of the arrow.
    mem.cgram[32 + 1] = rgb15(80, 144, 255);
    mem.cgram[32 + 2] = rgb15(48, 96, 208);
    mem.cgram[32 + 3] = rgb15(200, 224, 255);
    // BG3 2bpp sub-palette 3 (base = 3*4 = cgram[12..16]).
    mem.cgram[12 + 1] = rgb15(255, 255, 255);
    mem.cgram[12 + 2] = rgb15(144, 144, 144);
    mem.cgram[12 + 3] = rgb15(56, 56, 56);

    // ── Char data ────────────────────────────────────────────────────────
    // BG1/BG2 4bpp chars at 0x1000; BG2's 16x16 tile 4 = chars 4/5/20/21.
    put4(&mut mem, 1, ARROW);
    put4(&mut mem, 2, RING);
    put4(&mut mem, 4, block(6));
    put4(&mut mem, 5, block(7));
    put4(&mut mem, 20, block(8));
    put4(&mut mem, 21, block(9));
    // BG3 2bpp char 1 at 0x2000 (8 words/char).
    for (i, w) in pack_2bpp(STRIPE).into_iter().enumerate() {
        mem.vram[0x2000 + 8 + i] = w;
    }

    // ── Tilemaps ─────────────────────────────────────────────────────────
    // BG1 at 0x0000, 64x32 (screen 0 = 0x0000, screen 1 = 0x0400):
    // arrows marching diagonally with alternating flips/palettes in screen 0,
    // a coarse grid of priority-flagged rings in screen 1 (revealed by the
    // HDMA scroll band below).
    for i in 0..28usize {
        let (col, row) = ((i * 3) % 32, i);
        mem.vram[row * 32 + col] =
            entry(1, if i % 3 == 0 { 2 } else { 1 }, false, i % 2 == 1, i % 4 == 3);
    }
    for row in (1..28).step_by(6) {
        for col in (1..32).step_by(7) {
            mem.vram[0x0400 + row * 32 + col] = entry(2, 1, true, false, false);
        }
    }
    // BG2 at 0x0800: sparse 16x16 tiles — plain, H-flipped, V-flipped.
    mem.vram[0x0800 + 2 * 32 + 3] = entry(4, 1, false, false, false);
    mem.vram[0x0800 + 8 * 32 + 9] = entry(4, 1, false, true, false);
    mem.vram[0x0800 + 11 * 32 + 5] = entry(4, 1, false, false, true);
    // BG3 at 0x0c00: two stripe bands, the lower one flipping per column.
    for col in 0..32usize {
        mem.vram[0x0c00 + 20 * 32 + col] = entry(1, 3, false, false, false);
        mem.vram[0x0c00 + 21 * 32 + col] = entry(1, 3, true, col % 2 == 1, col % 4 == 2);
    }

    // ── Registers ────────────────────────────────────────────────────────
    let mut def = LineTableRow::default(); // Mode 1, brightness 15
    def.bg[0].char_base = 0x1000;
    def.bg[0].screen_size = 1; // 64x32
    def.bg[0].scroll_x = -5.0; // negative wrap
    def.bg[0].scroll_y = 3.0;
    def.bg[1].char_base = 0x1000;
    def.bg[1].map_base = 0x0800;
    def.bg[1].tile_size = 16;
    def.bg[2].char_base = 0x2000;
    def.bg[2].map_base = 0x0c00;
    def.bg[2].scroll_x = 8.0;
    let mut b = LineTableBuilder::new(def);
    // Bottom third: shove BG1 into tilemap screen 1 (the ring grid).
    b.hdma(150, 223, |_, r| {
        r.bg[0].scroll_x = 300.0;
    });
    (b.build(HEIGHT), mem)
}

fn decode_png(path: &str) -> Vec<u8> {
    let decoder = png::Decoder::new(std::fs::File::open(path).unwrap());
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    buf.truncate(info.buffer_size());
    buf
}

/// Structural sanity independent of the committed PNG: all three layers and
/// the backdrop actually contribute pixels (guards against a degenerate,
/// all-backdrop golden being silently frozen).
#[test]
fn bg_fixture_draws_all_three_layers() {
    let (lt, mem) = fixture();
    let fb = render_frame(&lt, &mem);
    let count = |c: [u8; 4]| fb.chunks(4).filter(|p| *p == c).count();
    assert!(count(unpack_rgb15(rgb15(255, 160, 48))) > 0, "BG1 arrow body missing");
    assert!(count(unpack_rgb15(rgb15(64, 200, 96))) > 0, "BG1 ring (screen 1) missing");
    assert!(count(unpack_rgb15(rgb15(40, 120, 216))) > 0, "BG2 16x16 block missing");
    assert!(count(unpack_rgb15(rgb15(255, 255, 255))) > 0, "BG3 stripe missing");
    assert!(count(unpack_rgb15(rgb15(16, 24, 48))) > 0, "backdrop missing");
}

#[test]
fn bg_fixture_matches_golden_png() {
    assert!(
        Path::new(GOLDEN).exists(),
        "golden missing — run: cargo test -p ppu-core regen_golden_bg -- --ignored"
    );
    let (lt, mem) = fixture();
    let actual = render_frame(&lt, &mem);
    let expected = decode_png(GOLDEN);
    assert_eq!(actual.len(), WIDTH * HEIGHT * 4);
    assert_eq!(actual, expected, "framebuffer differs from golden PNG");
}

#[test]
#[ignore = "regenerates the committed golden PNG"]
fn regen_golden_bg() {
    let (lt, mem) = fixture();
    let fb = render_frame(&lt, &mem);
    std::fs::create_dir_all("tests/fixtures").unwrap();
    let file = std::fs::File::create(GOLDEN).unwrap();
    let mut encoder = png::Encoder::new(file, WIDTH as u32, HEIGHT as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header().unwrap().write_image_data(&fb).unwrap();
}
