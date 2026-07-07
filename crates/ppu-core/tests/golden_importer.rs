//! Golden importer tests: known PNG -> exact char/tilemap/cgram bytes;
//! over-budget PNG -> honest overflow report; determinism.

use ppu_core::import::{import_tile_bg, ImportOptions, Overflow};

fn load_png(name: &str) -> (Vec<u8>, u32, u32) {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    let decoder = png::Decoder::new(std::fs::File::open(&path).unwrap());
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    buf.truncate(info.buffer_size());
    assert_eq!(info.color_type, png::ColorType::Rgba);
    (buf, info.width, info.height)
}

#[test]
fn golden_small_png_4bpp_bytes() {
    let (rgba, w, h) = load_png("importer_4bpp.png");
    let out = import_tile_bg(&rgba, w, h, &ImportOptions::default());
    // CGRAM: palette 0, sorted -> red at 1, blue at 2
    assert_eq!(out.cgram, vec![(1, 0x001f), (2, 0x7c00)]);
    // char: blank + all-red + half/half
    let mut expect_char = vec![0u16; 16]; // reserved blank tile 0
    expect_char.extend(std::iter::repeat(0x00ff).take(8)); // tile 1 planes 0/1
    expect_char.extend(std::iter::repeat(0x0000).take(8)); // tile 1 planes 2/3
    expect_char.extend(std::iter::repeat(0x0ff0).take(8)); // tile 2 planes 0/1
    expect_char.extend(std::iter::repeat(0x0000).take(8)); // tile 2 planes 2/3
    assert_eq!(out.char_words, expect_char);
    // tilemap: one 32x32 screen
    let mut expect_map = vec![0u16; 0x400];
    expect_map[0] = 0x0001;
    expect_map[1] = 0x0002;
    assert_eq!(out.tilemap_words, expect_map);
    assert_eq!(out.registers.map_base, 0x0000);
    assert_eq!(out.registers.char_base, 0x1000);
    assert_eq!(out.registers.screen_size, 0);
    assert_eq!(out.registers.tile_size, 8);
    assert!(out.report.overflows.is_empty());
    assert_eq!(out.report.vram_words, 48 + 0x400);
}

#[test]
fn golden_small_png_2bpp_bytes() {
    let (rgba, w, h) = load_png("importer_4bpp.png");
    let opts = ImportOptions {
        bit_depth: 2,
        ..Default::default()
    };
    let out = import_tile_bg(&rgba, w, h, &opts);
    assert_eq!(out.cgram, vec![(1, 0x001f), (2, 0x7c00)]); // 2bpp pal 0 base 0
    let mut expect_char = vec![0u16; 8];
    expect_char.extend(std::iter::repeat(0x00ff).take(8));
    expect_char.extend(std::iter::repeat(0x0ff0).take(8));
    assert_eq!(out.char_words, expect_char);
    assert_eq!(out.tilemap_words[0], 0x0001);
    assert_eq!(out.tilemap_words[1], 0x0002);
}

#[test]
fn overbudget_png_reports_palette_overflow_honestly() {
    let (rgba, w, h) = load_png("importer_overbudget.png");
    let out = import_tile_bg(&rgba, w, h, &ImportOptions::default());
    assert_eq!(out.report.palettes_used, 8); // hard cap respected
    assert_eq!(
        out.report.overflows,
        vec![Overflow::Palettes {
            needed: 9,
            remapped_tiles: 1
        }]
    );
    // every tile has the same index-grid relative to its own sorted palette,
    // so char dedup collapses all 9 cells onto one stored tile
    assert_eq!(out.report.unique_tiles, 1);
    // cells 0..8 use palettes 0..7 then fall back to best-overlap palette 0
    for k in 0..9usize {
        let (ty, tx) = (k / 3, k % 3);
        let word = out.tilemap_words[ty * 32 + tx];
        let pal = if k < 8 { k as u16 } else { 0 };
        assert_eq!(word & 0x03ff, 1, "cell {k} tile#");
        assert_eq!((word >> 10) & 7, pal, "cell {k} palette");
    }
    // sub-palettes stay within the 15-color capacity
    assert!(out.cgram.len() <= 8 * 15);
}

#[test]
fn import_is_deterministic() {
    for name in ["importer_4bpp.png", "importer_overbudget.png"] {
        let (rgba, w, h) = load_png(name);
        let a = import_tile_bg(&rgba, w, h, &ImportOptions::default());
        let b = import_tile_bg(&rgba, w, h, &ImportOptions::default());
        assert_eq!(a, b, "{name} must import identically every run");
    }
}
