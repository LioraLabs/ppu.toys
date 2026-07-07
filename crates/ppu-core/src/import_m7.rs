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
}
