//! Reusable, deterministic color-quantization primitives for importers.
//! All colors are packed BGR555 words; all math on their 5-bit channels.
//! Shared core: the Mode-7 importer (256-color) reuses `median_cut`/`nearest`;
//! OBJ import (4bpp) reuses everything incl. `region_fit`.

/// Split a BGR555 word into [r, g, b] 5-bit channels.
#[inline]
fn channels(c: u16) -> [i32; 3] {
    [
        (c & 0x1f) as i32,
        ((c >> 5) & 0x1f) as i32,
        ((c >> 10) & 0x1f) as i32,
    ]
}

/// Squared distance between two BGR555 colors in 5-bit RGB space.
#[inline]
pub fn dist2(a: u16, b: u16) -> i32 {
    let (ca, cb) = (channels(a), channels(b));
    (0..3).map(|i| (ca[i] - cb[i]).pow(2)).sum()
}

/// Index of the nearest palette color; ties break to the lowest index.
pub fn nearest(palette: &[u16], color: u16) -> usize {
    let mut best = 0;
    let mut bd = i32::MAX;
    for (i, &p) in palette.iter().enumerate() {
        let d = dist2(p, color);
        if d < bd {
            bd = d;
            best = i;
        }
    }
    best
}

/// Median-cut a weighted (bgr555, count) histogram down to <= `max` colors.
/// Deterministic: canonical (sorted) input, widest-channel box split (ties:
/// r,g,b then earliest box) at the weighted median, weighted-mean output.
/// Returns a sorted, deduped palette.
pub fn median_cut(hist: &[(u16, u32)], max: usize) -> Vec<u16> {
    let mut colors: Vec<(u16, u32)> = hist.to_vec();
    colors.sort_unstable();
    colors.dedup_by(|a, b| {
        if a.0 == b.0 {
            b.1 += a.1;
            true
        } else {
            false
        }
    });
    if colors.len() <= max {
        return colors.into_iter().map(|(c, _)| c).collect();
    }
    let mut boxes: Vec<Vec<(u16, u32)>> = vec![colors];
    while boxes.len() < max {
        // pick (box, channel) with the widest range; ties -> earliest box, r<g<b
        let mut pick = (0usize, 0usize);
        let mut pick_range = 0i32;
        for (bi, b) in boxes.iter().enumerate() {
            if b.len() < 2 {
                continue;
            }
            for ch in 0..3 {
                let (mut lo, mut hi) = (31, 0);
                for &(c, _) in b {
                    let v = channels(c)[ch];
                    lo = lo.min(v);
                    hi = hi.max(v);
                }
                if hi - lo > pick_range {
                    pick_range = hi - lo;
                    pick = (bi, ch);
                }
            }
        }
        if pick_range == 0 {
            break; // nothing splittable
        }
        let (bi, ch) = pick;
        let mut b = boxes.remove(bi);
        b.sort_unstable_by_key(|&(c, _)| (channels(c)[ch], c));
        let total: u64 = b.iter().map(|&(_, n)| n as u64).sum();
        let mut acc = 0u64;
        let mut split = b.len() - 1; // fallback: at least 1 element per side
        for (i, &(_, n)) in b.iter().enumerate() {
            acc += n as u64;
            if acc * 2 >= total {
                split = (i + 1).min(b.len() - 1);
                break;
            }
        }
        let hi_box = b.split_off(split);
        boxes.push(b);
        boxes.push(hi_box);
    }
    let mut out: Vec<u16> = boxes
        .iter()
        .map(|b| {
            let total: u64 = b.iter().map(|&(_, n)| n as u64).sum();
            let mut sum = [0u64; 3];
            for &(c, n) in b {
                let ch = channels(c);
                for i in 0..3 {
                    sum[i] += ch[i] as u64 * n as u64;
                }
            }
            let mean = |i: usize| ((sum[i] + total / 2) / total) as u16;
            (mean(2) << 10) | (mean(1) << 5) | mean(0)
        })
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// Result of the greedy multi-palette region fit.
pub struct RegionFit {
    /// Final sub-palettes: sorted, deduped, each len <= capacity.
    pub palettes: Vec<Vec<u16>>,
    /// Palette index assigned to each input tile.
    pub assignment: Vec<u8>,
    /// Palettes an uncapped greedy pass would have used (> max = overflow).
    pub palettes_needed: usize,
}

/// Greedy region fit: merge per-tile color sets (sorted, deduped, each already
/// <= capacity) into at most `max_palettes` palettes of `capacity` colors,
/// choosing the palette needing the fewest added colors (ties: lowest index).
/// Once no palette can take a tile and all slots are open, the tile is
/// assigned to the palette missing the fewest of its colors; the CALLER remaps
/// its pixels via `nearest` (honest overflow — sealed palettes never mutate
/// past capacity). `palettes_needed` counts an uncapped virtual overflow run
/// so the budget report can say "needs N palettes".
pub fn region_fit(tile_palettes: &[Vec<u16>], max_palettes: usize, capacity: usize) -> RegionFit {
    let mut palettes: Vec<Vec<u16>> = Vec::new();
    let mut virtual_extra: Vec<Vec<u16>> = Vec::new(); // uncapped shadow, for reporting
    let mut assignment = vec![0u8; tile_palettes.len()];
    let missing = |p: &[u16], tp: &[u16]| tp.iter().filter(|c| !p.contains(c)).count();
    for (ti, tp) in tile_palettes.iter().enumerate() {
        if tp.is_empty() {
            continue; // fully transparent tile: any palette (0) works
        }
        let mut best: Option<(usize, usize)> = None; // (added, palette idx)
        for (pi, p) in palettes.iter().enumerate() {
            let added = missing(p, tp);
            if p.len() + added <= capacity && best.map_or(true, |(ba, _)| added < ba) {
                best = Some((added, pi));
            }
        }
        match best {
            Some((_, pi)) => {
                let add: Vec<u16> =
                    tp.iter().copied().filter(|c| !palettes[pi].contains(c)).collect();
                palettes[pi].extend(add);
                palettes[pi].sort_unstable();
                assignment[ti] = pi as u8;
            }
            None if palettes.len() < max_palettes => {
                assignment[ti] = palettes.len() as u8;
                palettes.push(tp.clone());
            }
            None => {
                // best-overlap fallback among sealed palettes (fewest missing,
                // ties: lowest index); pixels get remapped by the caller.
                let pi = (0..palettes.len())
                    .min_by_key(|&pi| missing(&palettes[pi], tp))
                    .unwrap_or(0);
                assignment[ti] = pi as u8;
                // shadow bookkeeping for the honest "needs N palettes" count
                let vslot = virtual_extra.iter().position(|p| p.len() + missing(p, tp) <= capacity);
                match vslot {
                    Some(vi) => {
                        let add: Vec<u16> =
                            tp.iter().copied().filter(|c| !virtual_extra[vi].contains(c)).collect();
                        virtual_extra[vi].extend(add);
                        virtual_extra[vi].sort_unstable();
                    }
                    None => virtual_extra.push(tp.clone()),
                }
            }
        }
    }
    RegionFit {
        palettes_needed: palettes.len() + virtual_extra.len(),
        palettes,
        assignment,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_picks_min_distance_lowest_index_on_tie() {
        // palette: black, red31, blue31
        let pal = [0x0000u16, 0x001f, 0x7c00];
        assert_eq!(nearest(&pal, 0x001e), 1); // near-red -> red
        assert_eq!(nearest(&pal, 0x0000), 0); // exact
                                              // pal black(0,0,0) and (2,0,0): color (1,0,0) is d2=1 from both -> index 0
        let pal2 = [0x0000u16, 0x0002];
        assert_eq!(nearest(&pal2, 0x0001), 0);
    }

    #[test]
    fn median_cut_passthrough_when_under_budget() {
        let hist = [(0x001fu16, 4u32), (0x7c00, 1), (0x0000, 9)];
        assert_eq!(median_cut(&hist, 8), vec![0x0000, 0x001f, 0x7c00]); // sorted
    }

    #[test]
    fn median_cut_reduces_to_max_and_is_deterministic() {
        // 32 grays (r=g=b=v) -> ask for 4
        let hist: Vec<(u16, u32)> = (0..32u16).map(|v| ((v << 10) | (v << 5) | v, 1)).collect();
        let a = median_cut(&hist, 4);
        let b = median_cut(&hist, 4);
        assert_eq!(a, b);
        assert!(a.len() <= 4 && !a.is_empty());
        let mut s = a.clone();
        s.sort_unstable();
        s.dedup();
        assert_eq!(s, a); // sorted + deduped output contract
    }

    #[test]
    fn median_cut_input_order_does_not_matter() {
        let mut hist: Vec<(u16, u32)> = (0..32u16).map(|v| ((v << 10) | (v << 5) | v, 1)).collect();
        let a = median_cut(&hist, 5);
        hist.reverse();
        assert_eq!(median_cut(&hist, 5), a);
    }

    #[test]
    fn region_fit_merges_overlapping_tiles_into_one_palette() {
        // two tiles sharing colors, union fits capacity 15
        let t0 = vec![0x0001u16, 0x0002, 0x0003];
        let t1 = vec![0x0002u16, 0x0003, 0x0004];
        let fit = region_fit(&[t0, t1], 8, 15);
        assert_eq!(fit.palettes.len(), 1);
        assert_eq!(fit.assignment, vec![0, 0]);
        assert_eq!(fit.palettes[0], vec![0x0001, 0x0002, 0x0003, 0x0004]);
        assert_eq!(fit.palettes_needed, 1);
    }

    #[test]
    fn region_fit_opens_new_palette_when_capacity_would_overflow() {
        // capacity 3: t0 uses all 3; t1 disjoint -> second palette
        let t0 = vec![1u16, 2, 3];
        let t1 = vec![4u16, 5, 6];
        let fit = region_fit(&[t0, t1], 8, 3);
        assert_eq!(fit.palettes.len(), 2);
        assert_eq!(fit.assignment, vec![0, 1]);
    }

    #[test]
    fn region_fit_overflow_assigns_best_overlap_and_counts_needed() {
        // max 2 palettes of capacity 3; three mutually disjoint tiles + one
        // that overlaps t0 by 2 colors
        let tiles = vec![
            vec![1u16, 2, 3],
            vec![4u16, 5, 6],
            vec![7u16, 8, 9],    // no room -> overflow
            vec![2u16, 3, 10],   // also overflow; best overlap = palette 0
        ];
        let fit = region_fit(&tiles, 2, 3);
        assert_eq!(fit.palettes.len(), 2);
        assert_eq!(fit.assignment[0], 0);
        assert_eq!(fit.assignment[1], 1);
        assert_eq!(fit.assignment[3], 0); // overlap {2,3} beats palette 1's {}
        assert!(fit.palettes_needed > 2);
        // sealed palettes never exceed capacity
        assert!(fit.palettes.iter().all(|p| p.len() <= 3));
    }

    #[test]
    fn region_fit_empty_tile_palette_never_opens_a_palette() {
        let fit = region_fit(&[vec![], vec![1u16, 2]], 8, 3);
        assert_eq!(fit.palettes.len(), 1);
        assert_eq!(fit.assignment, vec![0, 0]);
    }
}
