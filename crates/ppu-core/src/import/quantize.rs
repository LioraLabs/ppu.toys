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
}
