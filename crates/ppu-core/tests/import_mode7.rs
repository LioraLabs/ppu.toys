//! Golden tests for the Mode 7 importer: known image -> exact interleaved VRAM
//! words + CGRAM; over-256-tile image -> honest overflow report; packing-level
//! round-trip (decode VRAM back to the quantized image).
//!
//! The render round-trip against the Mode 7 rasterizer is the M4 done gate —
//! the rasterizer is a stub on this branch while it is still in flight.

use ppu_core::{
    import_mode7, place_m7, rgb15, unpack_rgb15, Memory, SourceReport, M7_MAP_DIM, M7_MAX_TILES,
    M7_TILE_BYTES,
};

/// 16x16 four-quadrant image: red, red / green, blue. Two identical red
/// quadrants exercise dedup; 3 colors are under budget so the palette is the
/// exact sorted unique colors.
fn quadrant_rgba() -> Vec<u8> {
    let mut rgba = Vec::with_capacity(16 * 16 * 4);
    for y in 0..16 {
        for x in 0..16 {
            let c: [u8; 4] = match (x < 8, y < 8) {
                (_, true) => [255, 0, 0, 255],      // top: red, both quadrants
                (true, false) => [0, 255, 0, 255],  // bottom-left: green
                (false, false) => [0, 0, 255, 255], // bottom-right: blue
            };
            rgba.extend_from_slice(&c);
        }
    }
    rgba
}

#[test]
fn golden_known_image_exact_vram_and_cgram() {
    let (src, meta) = import_mode7(&quadrant_rgba(), 16, 16);
    let mut mem = Memory::new();
    place_m7(&src, &mut mem);

    // CGRAM: palette sorted by [u8;3] order -> blue, green, red at 1..=3.
    assert_eq!(mem.cgram[0], 0); // reserved transparent
    assert_eq!(mem.cgram[1], rgb15(0, 0, 255)); // 0x7c00
    assert_eq!(mem.cgram[2], rgb15(0, 255, 0)); // 0x03e0
    assert_eq!(mem.cgram[3], rgb15(255, 0, 0)); // 0x001f
    assert_eq!(&mem.cgram[4..], &[0u16; 252][..]);

    // Tiles in scan order: red (idx 3) = tile 0 (deduped), green (2) = tile 1,
    // blue (1) = tile 2. Char lane: tile t at byte offsets t*64..
    // Map lane: 2x2 map at the top-left of the 128-wide tilemap.
    assert_eq!(mem.vram[0], 0x0300); // char: tile0 px0 = 3; map (0,0) = tile 0
    assert_eq!(mem.vram[1], 0x0300); // map (1,0) also tile 0 (dedup)
    assert_eq!(mem.vram[63], 0x0300); // tile0 px63; map (63,0) empty = 0
    assert_eq!(mem.vram[64], 0x0200); // tile1 (green) px0; map (64,0) empty
    assert_eq!(mem.vram[128], 0x0101); // tile2 (blue) px0; map (0,1) = tile 1
    assert_eq!(mem.vram[129], 0x0102); // map (1,1) = tile 2
    assert_eq!(mem.vram[130], 0x0100); // map (2,1) beyond image = 0
    assert_eq!(mem.vram[192], 0x0000); // past all char data + map row 1

    // Report.
    let SourceReport::Mode7 { report } = &meta.report else {
        panic!("expected Mode7 report");
    };
    assert_eq!(report.colors, 3);
    assert_eq!(report.unique_tiles, 3);
    assert_eq!(report.tile_capacity, 256);
    assert_eq!(report.overflow_tiles, 0);
    assert_eq!((report.map_tiles_w, report.map_tiles_h), (2, 2));
}

#[test]
fn golden_overflow_reports_honestly() {
    // 512 unique tiles: tile n's 8x8 pattern encodes n in binary across its
    // first 9 pixels (white bit set / black bit clear) on an opaque black
    // background. 16x32 tiles = 128x256 px, 2 colors.
    let (w, h) = (128usize, 256usize);
    let mut rgba = vec![0u8; w * h * 4];
    for px in rgba.chunks_exact_mut(4) {
        px[3] = 255; // opaque black background
    }
    for ty in 0..(h / 8) {
        for tx in 0..(w / 8) {
            let n = ty * (w / 8) + tx;
            for bit in 0..9 {
                if n >> bit & 1 == 1 {
                    let (x, y) = (tx * 8 + bit % 8, ty * 8 + bit / 8);
                    let o = (y * w + x) * 4;
                    rgba[o..o + 4].copy_from_slice(&[255, 255, 255, 255]);
                }
            }
        }
    }
    let (src, meta) = import_mode7(&rgba, w, h);
    let SourceReport::Mode7 { report } = &meta.report else {
        panic!("expected Mode7 report");
    };
    assert_eq!(report.unique_tiles, 512);
    assert_eq!(report.overflow_tiles, 256);
    assert_eq!(report.tile_capacity, 256);
    assert_eq!((report.map_tiles_w, report.map_tiles_h), (16, 32));
    let mut mem = Memory::new();
    place_m7(&src, &mut mem);
    // Every map cell still holds a valid (<256) tile number; cells whose tile
    // fell past the budget hold 0. Scan order fills the budget in rows 0..15.
    for ty in 0..32 {
        for tx in 0..16 {
            let tile_no = (mem.vram[ty * M7_MAP_DIM + tx] & 0xff) as usize;
            assert!(tile_no < M7_MAX_TILES);
            if ty >= 16 {
                assert_eq!(tile_no, 0, "overflow cell ({tx},{ty}) must fall back to 0");
            }
        }
    }
}

#[test]
fn packing_round_trip_reconstructs_quantized_image() {
    // Decode VRAM back (map -> tile -> char pixel -> CGRAM color) and compare
    // to the source: pure primaries survive BGR555 exactly.
    let rgba = quadrant_rgba();
    let (src_data, _meta) = import_mode7(&rgba, 16, 16);
    let mut mem = Memory::new();
    place_m7(&src_data, &mut mem);
    for y in 0..16 {
        for x in 0..16 {
            let (tx, ty) = (x / 8, y / 8);
            let tile_no = (mem.vram[ty * M7_MAP_DIM + tx] & 0xff) as usize;
            let char_off = tile_no * M7_TILE_BYTES + (y % 8) * 8 + (x % 8);
            let idx = (mem.vram[char_off] >> 8) as usize;
            assert!(idx > 0, "opaque pixel must not be transparent index 0");
            let got = unpack_rgb15(mem.cgram[idx]);
            let src = &rgba[(y * 16 + x) * 4..(y * 16 + x) * 4 + 4];
            assert_eq!(&got[..3], &src[..3], "pixel ({x},{y})");
        }
    }
}
