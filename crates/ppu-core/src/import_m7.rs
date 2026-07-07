//! Mode 7 importer (m4/importer): converts a decoded RGBA image (a
//! `PpuCore.assets` entry) into the byte-interleaved Mode 7 VRAM region +
//! a single flat 256-color CGRAM palette.
//!
//! Pipeline: median-cut quantize whole image (<=255 opaque colors; CGRAM 0 is
//! reserved transparent) -> 8x8 split + exact dedup to <=256 unique tiles
//! (honest overflow report) -> interleave `vram[i] = (char<<8) | map`.
//!
//! `median_cut` / `nearest_color` / `dedup_tiles` are PRIVATE local helpers
//! that duplicate the shared importer core being built in parallel; at
//! integration these three get repointed at the shared module.

use std::collections::HashMap;

/// Mode 7 tilemap is 128x128 tile-number bytes (the low lane of words 0..0x4000).
pub const M7_MAP_DIM: usize = 128;
/// One Mode 7 tile: 8x8 linear 8bpp = 64 char bytes (the high lane).
pub const M7_TILE_BYTES: usize = 64;
/// 8-bit tile numbers: at most 256 unique tiles.
pub const M7_MAX_TILES: usize = 256;
/// The whole interleaved region: 16K words (map 128*128 = char 256*64 = 16384 bytes).
pub const M7_VRAM_WORDS: usize = 0x4000;

/// Median-cut quantize the opaque pixels (alpha >= 128) of an RGBA buffer to at
/// most `max_colors` RGB colors. Deterministic: unique colors are sorted before
/// bucketing, and when there are <= `max_colors` unique colors the palette is
/// exactly those colors in sorted order. Returned palette is sorted.
#[allow(dead_code)] // used by import_mode7 (next task)
fn median_cut(rgba: &[u8], max_colors: usize) -> Vec<[u8; 3]> {
    let mut counts: HashMap<[u8; 3], u32> = HashMap::new();
    for px in rgba.chunks_exact(4) {
        if px[3] >= 128 {
            *counts.entry([px[0], px[1], px[2]]).or_insert(0) += 1;
        }
    }
    let mut colors: Vec<([u8; 3], u32)> = counts.into_iter().collect();
    colors.sort_unstable(); // HashMap iteration order is random; goldens need determinism
    if colors.is_empty() {
        return Vec::new();
    }
    if colors.len() <= max_colors {
        return colors.into_iter().map(|(c, _)| c).collect();
    }
    // Median cut: repeatedly split the bucket with the widest single-channel
    // range at its median along that channel.
    let mut buckets: Vec<Vec<([u8; 3], u32)>> = vec![colors];
    while buckets.len() < max_colors {
        let mut best: Option<(usize, usize, u8)> = None; // (bucket, channel, range)
        for (i, b) in buckets.iter().enumerate() {
            if b.len() < 2 {
                continue;
            }
            for ch in 0..3 {
                let lo = b.iter().map(|(c, _)| c[ch]).min().unwrap();
                let hi = b.iter().map(|(c, _)| c[ch]).max().unwrap();
                if best.map_or(true, |(_, _, r)| hi - lo > r) {
                    best = Some((i, ch, hi - lo));
                }
            }
        }
        let Some((i, ch, _)) = best else { break }; // nothing splittable left
        let mut lo = buckets.swap_remove(i);
        lo.sort_unstable_by_key(|(c, _)| c[ch]);
        let hi = lo.split_off(lo.len() / 2);
        buckets.push(lo);
        buckets.push(hi);
    }
    // Palette entry = count-weighted mean of each bucket.
    let mut palette: Vec<[u8; 3]> = buckets
        .iter()
        .map(|b| {
            let total: u64 = b.iter().map(|&(_, n)| n as u64).sum();
            let mut sum = [0u64; 3];
            for (c, n) in b {
                for ch in 0..3 {
                    sum[ch] += c[ch] as u64 * *n as u64;
                }
            }
            [(sum[0] / total) as u8, (sum[1] / total) as u8, (sum[2] / total) as u8]
        })
        .collect();
    palette.sort_unstable();
    palette
}

/// Nearest palette entry by squared RGB distance; the lowest index wins ties.
#[allow(dead_code)] // used by import_mode7 (next task)
fn nearest_color(palette: &[[u8; 3]], c: [u8; 3]) -> u8 {
    let mut best = 0usize;
    let mut best_d = u32::MAX;
    for (i, p) in palette.iter().enumerate() {
        let d: u32 = (0..3)
            .map(|ch| {
                let d = p[ch] as i32 - c[ch] as i32;
                (d * d) as u32
            })
            .sum();
        if d < best_d {
            best_d = d;
            best = i;
        }
    }
    best as u8
}

/// Split an indexed image (one palette index byte per pixel, row-major) into
/// 8x8 tiles in scan order, exact-dedup them, and build the tilemap. Partial
/// edge tiles are padded with index 0. Unique tiles beyond `max_tiles` fall
/// back to tile 0 in the map (the honest-overflow policy); the true unique
/// count is returned so the caller can report the overflow.
///
/// Returns `(unique_tiles, map, tiles_w, tiles_h, unique_total)` where `map`
/// is `tiles_w * tiles_h` tile-number bytes.
#[allow(dead_code)] // used by import_mode7 (next task)
fn dedup_tiles(
    indexed: &[u8],
    width: usize,
    height: usize,
    max_tiles: usize,
) -> (Vec<[u8; M7_TILE_BYTES]>, Vec<u8>, usize, usize, usize) {
    let tiles_w = width.div_ceil(8).min(M7_MAP_DIM);
    let tiles_h = height.div_ceil(8).min(M7_MAP_DIM);
    let mut uniq: Vec<[u8; M7_TILE_BYTES]> = Vec::new();
    let mut ids: HashMap<[u8; M7_TILE_BYTES], usize> = HashMap::new();
    let mut map = vec![0u8; tiles_w * tiles_h];
    for ty in 0..tiles_h {
        for tx in 0..tiles_w {
            let mut t = [0u8; M7_TILE_BYTES];
            for py in 0..8 {
                for px in 0..8 {
                    let (x, y) = (tx * 8 + px, ty * 8 + py);
                    if x < width && y < height {
                        t[py * 8 + px] = indexed[y * width + x];
                    }
                }
            }
            let next = ids.len();
            let id = *ids.entry(t).or_insert(next);
            if id == next && id < max_tiles {
                uniq.push(t); // first sighting, still under budget
            }
            map[ty * tiles_w + tx] = if id < max_tiles { id as u8 } else { 0 };
        }
    }
    let unique_total = ids.len();
    (uniq, map, tiles_w, tiles_h, unique_total)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build an RGBA buffer from a list of (r,g,b) pixels (all opaque).
    fn rgba_of(pixels: &[[u8; 3]]) -> Vec<u8> {
        pixels.iter().flat_map(|c| [c[0], c[1], c[2], 255]).collect()
    }

    #[test]
    fn median_cut_exact_when_under_budget() {
        // 3 unique colors, budget 255 -> palette is exactly those, sorted.
        let rgba = rgba_of(&[[255, 0, 0], [0, 255, 0], [0, 0, 255], [255, 0, 0]]);
        let pal = median_cut(&rgba, 255);
        assert_eq!(pal, vec![[0, 0, 255], [0, 255, 0], [255, 0, 0]]);
    }

    #[test]
    fn median_cut_ignores_transparent_pixels() {
        let mut rgba = rgba_of(&[[10, 20, 30]]);
        rgba.extend_from_slice(&[200, 200, 200, 0]); // alpha 0 -> ignored
        let pal = median_cut(&rgba, 255);
        assert_eq!(pal, vec![[10, 20, 30]]);
    }

    #[test]
    fn median_cut_reduces_to_budget() {
        // 256 unique grays, budget 4 -> exactly 4 entries, deterministic.
        let pixels: Vec<[u8; 3]> = (0..=255u8).map(|v| [v, v, v]).collect();
        let rgba = rgba_of(&pixels);
        let pal = median_cut(&rgba, 4);
        assert_eq!(pal.len(), 4);
        assert_eq!(pal, median_cut(&rgba, 4)); // deterministic across runs
        // Buckets are 64-gray runs; count-weighted means truncate: 31.5 -> 31, ...
        assert_eq!(pal, vec![[31, 31, 31], [95, 95, 95], [159, 159, 159], [223, 223, 223]]);
    }

    #[test]
    fn median_cut_empty_image_is_empty_palette() {
        assert!(median_cut(&[], 255).is_empty());
        assert!(median_cut(&[0, 0, 0, 0], 255).is_empty()); // one transparent px
    }

    #[test]
    fn nearest_color_picks_min_distance_lowest_index_ties() {
        let pal = [[0, 0, 0], [100, 100, 100], [200, 200, 200]];
        assert_eq!(nearest_color(&pal, [10, 10, 10]), 0);
        assert_eq!(nearest_color(&pal, [190, 190, 190]), 2);
        assert_eq!(nearest_color(&pal, [50, 50, 50]), 0); // equidistant -> lowest index
    }

    #[test]
    fn dedup_tiles_dedups_and_maps_in_scan_order() {
        // 16x16 indexed image, 4 quadrant tiles: A A / B C.
        let mut indexed = vec![0u8; 16 * 16];
        for y in 0..16 {
            for x in 0..16 {
                indexed[y * 16 + x] = match (x < 8, y < 8) {
                    (_, true) => 7,      // top half: two identical tiles of index 7
                    (true, false) => 3,  // bottom-left
                    (false, false) => 5, // bottom-right
                };
            }
        }
        let (uniq, map, tw, th, total) = dedup_tiles(&indexed, 16, 16, M7_MAX_TILES);
        assert_eq!((tw, th, total), (2, 2, 3));
        assert_eq!(map, vec![0, 0, 1, 2]); // top tiles dedup to id 0
        assert_eq!(uniq.len(), 3);
        assert_eq!(uniq[0], [7u8; 64]);
        assert_eq!(uniq[1], [3u8; 64]);
        assert_eq!(uniq[2], [5u8; 64]);
    }

    #[test]
    fn dedup_tiles_pads_partial_edge_tiles_with_zero() {
        // 4x4 image of index 9 -> one tile, right/bottom padded with 0.
        let indexed = vec![9u8; 16];
        let (uniq, map, tw, th, total) = dedup_tiles(&indexed, 4, 4, M7_MAX_TILES);
        assert_eq!((tw, th, total), (1, 1, 1));
        assert_eq!(map, vec![0]);
        let t = uniq[0];
        assert_eq!(t[0], 9); // (0,0) inside image
        assert_eq!(t[4], 0); // (4,0) padding
        assert_eq!(t[4 * 8], 0); // (0,4) padding
    }

    #[test]
    fn dedup_tiles_overflow_falls_back_to_tile_zero_but_counts_honestly() {
        // 3 unique tiles, budget 2 -> third maps to 0, unique_total still 3.
        let mut indexed = vec![0u8; 24 * 8]; // 3 tiles in a row
        for y in 0..8 {
            for x in 8..16 {
                indexed[y * 24 + x] = 1;
            }
            for x in 16..24 {
                indexed[y * 24 + x] = 2;
            }
        }
        let (uniq, map, _, _, total) = dedup_tiles(&indexed, 24, 8, 2);
        assert_eq!(total, 3);
        assert_eq!(uniq.len(), 2); // only the first two kept
        assert_eq!(map, vec![0, 1, 0]); // overflow tile -> 0
    }

    #[test]
    fn dedup_tiles_clamps_map_to_128() {
        // 1032px wide (129 tiles) x 8 -> map clamped to 128 wide.
        let indexed = vec![1u8; 1032 * 8];
        let (_, map, tw, th, _) = dedup_tiles(&indexed, 1032, 8, M7_MAX_TILES);
        assert_eq!((tw, th), (128, 1));
        assert_eq!(map.len(), 128);
    }
}
