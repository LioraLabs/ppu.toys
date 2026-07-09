//! Golden OBJ per-line limit + size-pair scenes over hand-authored OBJ VRAM.
//!
//! Solid index-1 char tiles packed at the OBSEL base so any sprite renders as a
//! clean filled rectangle; the scenes isolate (1) a small-vs-large size pair from
//! one size_sel, (2) the deterministic >32-sprite range-cap drop with rotation
//! OFF (lowest indices win), and (3) the same scene with rotation ON (OAMADD start
//! shifts the surviving set). Structural assertions on bin_line + framebuffer, plus
//! PNG compare, following the golden_sprite.rs / golden_composite.rs conventions.
use ppu_core::{
    bin_line, render_frame, rgb15, unpack_rgb15, LineTableBuilder, LineTableRow, Memory, Obj,
    HEIGHT, WIDTH,
};
use std::path::Path;

const CHAR_BASE: usize = 0x2000;
const SIZE_PAIR_GOLDEN: &str = "tests/fixtures/golden_obj_size_pair.png";
const OVER_LIMIT_GOLDEN: &str = "tests/fixtures/golden_obj_over_limit.png";
const ROTATION_GOLDEN: &str = "tests/fixtures/golden_obj_rotation.png";

/// Write a solid 4bpp OBJ char (index 1) at `CHAR_BASE`, tile `n` (plane 0 = 0xFF).
fn put_solid(mem: &mut Memory, n: usize) {
    let base = CHAR_BASE + n * 16;
    for y in 0..8 {
        mem.vram[base + y] = 0x00ff; // plane 0 all set, planes 1..3 clear -> index 1
    }
}

fn px(fb: &[u8], x: usize, y: usize) -> [u8; 4] {
    let o = (y * WIDTH + x) * 4;
    [fb[o], fb[o + 1], fb[o + 2], fb[o + 3]]
}

fn decode_png(path: &str) -> Vec<u8> {
    let decoder = png::Decoder::new(std::fs::File::open(path).unwrap());
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    buf.truncate(info.buffer_size());
    buf
}

fn write_png(path: &str, fb: &[u8]) {
    std::fs::create_dir_all("tests/fixtures").unwrap();
    let file = std::fs::File::create(path).unwrap();
    let mut encoder = png::Encoder::new(file, WIDTH as u32, HEIGHT as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header().unwrap().write_image_data(fb).unwrap();
}

/// size_sel 2: small 8x8 / large 64x64 — dramatic pair from ONE selector.
fn size_pair_fixture() -> (ppu_core::LineTable, Memory) {
    let mut mem = Memory::new();
    mem.obsel.char_base = CHAR_BASE as u16;
    mem.obsel.size_sel = 2;
    mem.cgram[0] = rgb15(16, 16, 32); // backdrop
    mem.cgram[128 + 1] = rgb15(255, 80, 80); // pal 0 (small)
    mem.cgram[128 + 16 + 1] = rgb15(80, 160, 255); // pal 1 (large)
    // 64x64 samples an 8x8 tile block reaching tile 119; fill 0..128 solid.
    for n in 0..128usize {
        put_solid(&mut mem, n);
    }
    mem.oam[0] = Obj { on: true, x: 16, y: 88, tile: 0, pal: 0, large: false, ..Obj::default() }; // 8x8
    mem.oam[1] = Obj { on: true, x: 120, y: 40, tile: 0, pal: 1, large: true, ..Obj::default() }; // 64x64
    (LineTableBuilder::new(LineTableRow::default()).build(HEIGHT), mem)
}

#[test]
fn size_pair_small_is_8x8_and_large_is_64x64() {
    let (lt, mem) = size_pair_fixture();
    let fb = render_frame(&lt, &mem);
    let small = unpack_rgb15(rgb15(255, 80, 80));
    let large = unpack_rgb15(rgb15(80, 160, 255));
    // Small sprite: filled 8x8, and exactly 8 wide / 8 tall.
    assert_eq!(px(&fb, 16, 88), small, "small top-left");
    assert_eq!(px(&fb, 23, 95), small, "small fills 8x8 (x+7,y+7)");
    assert_ne!(px(&fb, 24, 88), small, "small must be only 8 px wide");
    assert_ne!(px(&fb, 16, 96), small, "small must be only 8 px tall");
    // Large sprite: filled 64x64, and exactly 64 wide.
    assert_eq!(px(&fb, 120, 40), large, "large top-left");
    assert_eq!(px(&fb, 183, 103), large, "large fills 64x64 (x+63,y+63)");
    assert_ne!(px(&fb, 184, 40), large, "large must be only 64 px wide");
}

#[test]
fn size_pair_matches_golden_png() {
    assert!(
        Path::new(SIZE_PAIR_GOLDEN).exists(),
        "golden missing — run: cargo test -p ppu-core regen_golden_obj_size_pair -- --ignored"
    );
    let (lt, mem) = size_pair_fixture();
    assert_eq!(render_frame(&lt, &mem), decode_png(SIZE_PAIR_GOLDEN));
}

#[test]
#[ignore = "regenerates the committed OBJ size-pair golden PNG"]
fn regen_golden_obj_size_pair() {
    let (lt, mem) = size_pair_fixture();
    write_png(SIZE_PAIR_GOLDEN, &render_frame(&lt, &mem));
}

/// 40 8x8 sprites on one Y band, index<->column. `rotate` moves the OAMADD start
/// to sprite 8. size_sel 0 -> 1 sliver each -> the 32-sprite RANGE cap binds.
fn storm_fixture(rotate: bool) -> (ppu_core::LineTable, Memory) {
    let mut mem = Memory::new();
    mem.obsel.char_base = CHAR_BASE as u16;
    mem.obsel.size_sel = 0;
    mem.cgram[0] = rgb15(16, 16, 32); // backdrop
    let pal_colors = [
        rgb15(255, 80, 80),
        rgb15(255, 180, 60),
        rgb15(240, 240, 80),
        rgb15(80, 220, 100),
        rgb15(80, 200, 220),
        rgb15(90, 130, 255),
        rgb15(190, 100, 240),
        rgb15(230, 230, 230),
    ];
    for (p, &c) in pal_colors.iter().enumerate() {
        mem.cgram[128 + p * 16 + 1] = c;
    }
    put_solid(&mut mem, 0); // 8x8 needs only tile 0
    for i in 0..40usize {
        mem.oam[i] = Obj {
            on: true,
            x: (6 + i * 6) as i16,
            y: 100,
            tile: 0,
            pal: (i % 8) as u8,
            large: false,
            ..Obj::default()
        };
    }
    if rotate {
        mem.priority_rotate = true;
        mem.oam_addr = 8 << 1; // obj_first_sprite(16) = 8 -> eval starts at index 8
    }
    (LineTableBuilder::new(LineTableRow::default()).build(HEIGHT), mem)
}

#[test]
fn over_limit_drops_highest_indices_with_rotation_off() {
    let (lt, mem) = storm_fixture(false);
    let bin = bin_line(&mem, 100);
    assert_eq!(bin.sprites.len(), 32);
    assert_eq!(bin.sprites.first(), Some(&0));
    assert_eq!(bin.sprites.last(), Some(&31));
    assert!(bin.range_over, "40 > 32 sprites in range");
    assert!(!bin.time_over, "32 slivers < 34 tile budget");
    let fb = render_frame(&lt, &mem);
    let backdrop = unpack_rgb15(rgb15(16, 16, 32));
    assert_ne!(px(&fb, 6, 100), backdrop, "index 0 kept (x=6)");
    assert_ne!(px(&fb, 192, 100), backdrop, "index 31 kept (x=192)");
    assert_eq!(px(&fb, 240, 100), backdrop, "index 39 dropped (rotation off)");
}

#[test]
fn rotation_shifts_the_surviving_set() {
    let (lt, mem) = storm_fixture(true);
    let bin = bin_line(&mem, 100);
    assert_eq!(bin.sprites.len(), 32);
    assert_eq!(bin.sprites.first(), Some(&8), "eval starts at the OAMADD sprite");
    assert!(!bin.sprites.contains(&0) && !bin.sprites.contains(&7), "indices 0..7 dropped");
    assert!(bin.sprites.contains(&39), "index 39 now survives");
    assert!(bin.range_over);
    let fb = render_frame(&lt, &mem);
    let backdrop = unpack_rgb15(rgb15(16, 16, 32));
    assert_eq!(px(&fb, 6, 100), backdrop, "index 0 dropped once the start rotates to 8");
    assert_ne!(px(&fb, 240, 100), backdrop, "index 39 kept (opposite of rotation-off)");
}

#[test]
fn over_limit_matches_golden_png() {
    assert!(
        Path::new(OVER_LIMIT_GOLDEN).exists(),
        "golden missing — run: cargo test -p ppu-core regen_golden_obj_over_limit -- --ignored"
    );
    let (lt, mem) = storm_fixture(false);
    assert_eq!(render_frame(&lt, &mem), decode_png(OVER_LIMIT_GOLDEN));
}

#[test]
fn rotation_matches_golden_png() {
    assert!(
        Path::new(ROTATION_GOLDEN).exists(),
        "golden missing — run: cargo test -p ppu-core regen_golden_obj_rotation -- --ignored"
    );
    let (lt, mem) = storm_fixture(true);
    assert_eq!(render_frame(&lt, &mem), decode_png(ROTATION_GOLDEN));
}

#[test]
#[ignore = "regenerates the committed OBJ over-limit golden PNG"]
fn regen_golden_obj_over_limit() {
    let (lt, mem) = storm_fixture(false);
    write_png(OVER_LIMIT_GOLDEN, &render_frame(&lt, &mem));
}

#[test]
#[ignore = "regenerates the committed OBJ rotation golden PNG"]
fn regen_golden_obj_rotation() {
    let (lt, mem) = storm_fixture(true);
    write_png(ROTATION_GOLDEN, &render_frame(&lt, &mem));
}
