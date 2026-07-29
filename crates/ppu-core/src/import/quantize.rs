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

/// Perceptually weighted squared distance between two BGR555 colors in 5-bit
/// RGB space. Luma-ish channel weights (r2 g4 b3): the eye resolves green
/// error hardest, blue softest — plain Euclidean trades green banding for
/// blue accuracy nobody can see.
#[inline]
pub fn dist2(a: u16, b: u16) -> i32 {
    let (ca, cb) = (channels(a), channels(b));
    2 * (ca[0] - cb[0]).pow(2) + 4 * (ca[1] - cb[1]).pow(2) + 3 * (ca[2] - cb[2]).pow(2)
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

/// A few Lloyd iterations over a weighted histogram: assign every color to its
/// nearest palette entry, then move each entry to its cluster's weighted mean.
/// Refines the coarse spread `median_cut` picks without changing the budget.
/// Deterministic (fixed iteration order, `nearest` tie-breaks); early-exits on
/// convergence. Returns sorted + deduped (the shared palette contract).
pub fn kmeans_refine(hist: &[(u16, u32)], palette: &[u16], iters: usize) -> Vec<u16> {
    let mut pal: Vec<u16> = palette.to_vec();
    for _ in 0..iters {
        if pal.is_empty() {
            break;
        }
        let mut sums = vec![[0u64; 4]; pal.len()]; // r, g, b, weight
        for &(c, n) in hist {
            let k = nearest(&pal, c);
            let ch = channels(c);
            for i in 0..3 {
                sums[k][i] += ch[i] as u64 * n as u64;
            }
            sums[k][3] += n as u64;
        }
        let next: Vec<u16> = pal
            .iter()
            .enumerate()
            .map(|(k, &old)| {
                let s = &sums[k];
                if s[3] == 0 {
                    old // empty cluster: keep the entry rather than invent one
                } else {
                    let mean = |i: usize| ((s[i] + s[3] / 2) / s[3]) as u16;
                    (mean(2) << 10) | (mean(1) << 5) | mean(0)
                }
            })
            .collect();
        if next == pal {
            break;
        }
        pal = next;
    }
    pal.sort_unstable();
    pal.dedup();
    pal
}

/// Budget a weighted histogram to <= `max` colors: median-cut for the spread,
/// k-means for the polish. The one call every over-budget reduction should use.
pub fn reduce_palette(hist: &[(u16, u32)], max: usize) -> Vec<u16> {
    let pal = median_cut(hist, max);
    kmeans_refine(hist, &pal, 8)
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
                let add: Vec<u16> = tp
                    .iter()
                    .copied()
                    .filter(|c| !palettes[pi].contains(c))
                    .collect();
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
                let vslot = virtual_extra
                    .iter()
                    .position(|p| p.len() + missing(p, tp) <= capacity);
                match vslot {
                    Some(vi) => {
                        let add: Vec<u16> = tp
                            .iter()
                            .copied()
                            .filter(|c| !virtual_extra[vi].contains(c))
                            .collect();
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

/// Cluster-and-refine multi-palette fit — the quality successor to plain
/// `region_fit`. Input is one weighted (sorted-by-color) histogram per tile.
///
/// Seed: tiles ordered color-richest-first (ties: input order) through the
/// greedy `region_fit`, so big palettes claim space before leftovers fragment
/// it. Refine: Lloyd iterations at the palette level — (a) reassign every tile
/// to the sub-palette with the least weighted pixel remap error, (b) rebuild
/// each sub-palette from its assigned tiles' merged histograms (exact union
/// when it fits, `reduce_palette` when it doesn't). Overflow tiles therefore
/// land where they *look* best, not where set overlap said. Deterministic:
/// fixed iteration order, ties to the lowest palette index.
///
/// `palettes_needed` reports the seed's uncapped greedy count (the honest
/// "needs N palettes" number); unused palettes are pruned at the end.
pub fn fit_palettes(
    tile_hists: &[Vec<(u16, u32)>],
    max_palettes: usize,
    capacity: usize,
) -> RegionFit {
    // Per-tile color sets for seeding; over-cap tiles get a weighted reduction
    // (the old path reduced with every color weighted 1).
    let sets: Vec<Vec<u16>> = tile_hists
        .iter()
        .map(|h| {
            if h.len() > capacity {
                reduce_palette(h, capacity)
            } else {
                h.iter().map(|&(c, _)| c).collect() // hists are color-sorted
            }
        })
        .collect();
    let mut order: Vec<usize> = (0..sets.len()).collect();
    order.sort_by_key(|&i| (usize::MAX - sets[i].len(), i));
    let ordered: Vec<Vec<u16>> = order.iter().map(|&i| sets[i].clone()).collect();
    let seed = region_fit(&ordered, max_palettes, capacity);
    let mut assignment = vec![0u8; sets.len()];
    for (k, &i) in order.iter().enumerate() {
        assignment[i] = seed.assignment[k];
    }
    let mut palettes = seed.palettes;

    for _ in 0..8 {
        // (a) reassign each tile to the palette that remaps it with least error
        let mut changed = false;
        for (ti, h) in tile_hists.iter().enumerate() {
            if h.is_empty() {
                continue;
            }
            let mut best = (i64::MAX, assignment[ti] as usize);
            for (pi, p) in palettes.iter().enumerate() {
                if p.is_empty() {
                    continue;
                }
                let err: i64 = h
                    .iter()
                    .map(|&(c, n)| dist2(p[nearest(p, c)], c) as i64 * n as i64)
                    .sum();
                if err < best.0 {
                    best = (err, pi);
                }
            }
            if best.1 != assignment[ti] as usize {
                assignment[ti] = best.1 as u8;
                changed = true;
            }
        }
        // (b) rebuild each palette from its tiles' merged pixel histograms
        let mut next: Vec<Vec<u16>> = Vec::with_capacity(palettes.len());
        for pi in 0..palettes.len() {
            let mut merged: std::collections::BTreeMap<u16, u32> = Default::default();
            for (ti, h) in tile_hists.iter().enumerate() {
                if assignment[ti] as usize == pi {
                    for &(c, n) in h {
                        *merged.entry(c).or_default() += n;
                    }
                }
            }
            if merged.is_empty() {
                next.push(palettes[pi].clone());
                continue;
            }
            let hv: Vec<(u16, u32)> = merged.into_iter().collect();
            next.push(if hv.len() <= capacity {
                hv.iter().map(|&(c, _)| c).collect()
            } else {
                reduce_palette(&hv, capacity)
            });
        }
        if !changed && next == palettes {
            break;
        }
        palettes = next;
    }

    // prune palettes no tile ended up using (empty-hist tiles pin to 0)
    let mut used = vec![false; palettes.len()];
    for (ti, h) in tile_hists.iter().enumerate() {
        if !h.is_empty() {
            used[assignment[ti] as usize] = true;
        }
    }
    if used.iter().any(|&u| !u) {
        let mut remap = vec![0u8; palettes.len()];
        let mut kept: Vec<Vec<u16>> = Vec::new();
        for (pi, p) in palettes.into_iter().enumerate() {
            if used[pi] {
                remap[pi] = kept.len() as u8;
                kept.push(p);
            }
        }
        for (ti, h) in tile_hists.iter().enumerate() {
            assignment[ti] = if h.is_empty() {
                0
            } else {
                remap[assignment[ti] as usize]
            };
        }
        palettes = kept;
    }

    RegionFit {
        palettes_needed: seed.palettes_needed,
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
            vec![7u16, 8, 9],  // no room -> overflow
            vec![2u16, 3, 10], // also overflow; best overlap = palette 0
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

    #[test]
    fn kmeans_refine_moves_entry_to_weighted_mean_and_keeps_contract() {
        // one entry, two reds weighted 9:1 -> mean r = (0*9 + 4*1 + 5) / 10 = 0
        let hist = [(0x0000u16, 9u32), (0x0004, 1)];
        assert_eq!(kmeans_refine(&hist, &[0x0002], 8), vec![0x0000]);
        // an empty cluster keeps its entry (never invents or drops a slot);
        // output is sorted regardless of input order
        let hist2 = [(0x0001u16, 1u32)];
        assert_eq!(
            kmeans_refine(&hist2, &[0x0003, 0x0000], 8),
            vec![0x0001, 0x0003]
        );
    }

    #[test]
    fn reduce_palette_is_deterministic_and_within_budget() {
        let hist: Vec<(u16, u32)> = (0..32u16)
            .map(|v| ((v << 10) | (v << 5) | v, 1 + v as u32))
            .collect();
        let a = reduce_palette(&hist, 4);
        assert_eq!(a, reduce_palette(&hist, 4));
        assert!(a.len() <= 4 && !a.is_empty());
        let mut s = a.clone();
        s.sort_unstable();
        s.dedup();
        assert_eq!(s, a);
    }

    #[test]
    fn fit_palettes_rebuilds_toward_heavy_pixels() {
        // capacity 1, one palette: r=29 x99 pixels vs r=31 x1 -> the rebuilt
        // entry sits at the weighted mean (29), not first-come (31).
        let t0 = vec![(0x001fu16, 1u32)];
        let t1 = vec![(0x001du16, 99u32)];
        let fit = fit_palettes(&[t0, t1], 1, 1);
        assert_eq!(fit.palettes, vec![vec![0x001d]]);
        assert_eq!(fit.assignment, vec![0, 0]);
        assert_eq!(fit.palettes_needed, 2); // honest seed-time overflow count
    }

    #[test]
    fn fit_palettes_seeds_color_rich_tiles_first_and_restores_order() {
        // capacity 3, disjoint: the 3-color tile seeds palette 0 even though
        // it arrives second; assignments still index the input order.
        let small = vec![(1u16, 1u32)];
        let big = vec![(4u16, 1u32), (5, 1), (6, 1)];
        let fit = fit_palettes(&[small, big], 2, 3);
        assert_eq!(fit.assignment, vec![1, 0]);
        assert_eq!(fit.palettes[0], vec![4, 5, 6]);
        assert_eq!(fit.palettes[1], vec![1]);
    }

    #[test]
    fn fit_palettes_exact_union_when_tiles_share_colors() {
        let t0 = vec![(1u16, 1u32), (2, 1), (3, 1)];
        let t1 = vec![(2u16, 1u32), (3, 1), (4, 1)];
        let fit = fit_palettes(&[t0, t1], 8, 15);
        assert_eq!(fit.palettes.len(), 1);
        assert_eq!(fit.palettes[0], vec![1, 2, 3, 4]);
        assert_eq!(fit.palettes_needed, 1);
    }

    #[test]
    fn fit_palettes_empty_tiles_pin_to_palette_zero() {
        let fit = fit_palettes(&[vec![], vec![(1u16, 1u32)]], 8, 3);
        assert_eq!(fit.palettes.len(), 1);
        assert_eq!(fit.assignment, vec![0, 0]);
    }
}
