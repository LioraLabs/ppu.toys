//! Final palette remap of source pixels into tile index grids, with optional
//! dithering. Runs on the ORIGINAL 8-bit RGBA (pre-BGR555) so the 555
//! truncation is dithered away with everything else; palette entries are
//! expanded 5->8 bit for the comparison. Shared by the tile-BG and OBJ
//! importers — palette *generation* stays in `quantize`, this is only how
//! pixels land in the palettes it produced.

use super::tiles::IndexTile;

/// How pixels are matched to their sub-palette.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum DitherMode {
    /// Flat nearest-color. Best for pixel art that is already palette-shaped.
    #[default]
    None,
    /// 8x8 ordered Bayer threshold. Position-stable (identical tiles still
    /// dedup) and the authentic SNES-era look; the default for photos.
    Bayer,
    /// Serpentine Floyd-Steinberg error diffusion. Smoothest gradients, but
    /// noise breaks tile dedup — expect the unique-tile count to jump.
    Diffusion,
}

/// Remap-stage options; extends `ImportOptions`/OBJ import without giving the
/// palette stages more knobs than they have.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RemapOptions {
    pub dither: DitherMode,
    /// Dither intensity 0-100. Bayer: threshold amplitude; diffusion: how much
    /// of the error is carried.
    pub strength: u8,
    /// Source alpha >= this is opaque. Must match the split/palette stages.
    pub alpha_threshold: u8,
}

impl Default for RemapOptions {
    fn default() -> Self {
        RemapOptions {
            dither: DitherMode::None,
            strength: 50,
            alpha_threshold: 128,
        }
    }
}

/// Classic 8x8 Bayer matrix, values 0..=63.
const BAYER8: [[i32; 8]; 8] = [
    [0, 32, 8, 40, 2, 34, 10, 42],
    [48, 16, 56, 24, 50, 18, 58, 26],
    [12, 44, 4, 36, 14, 46, 6, 38],
    [60, 28, 52, 20, 62, 30, 54, 22],
    [3, 35, 11, 43, 1, 33, 9, 41],
    [51, 19, 59, 27, 49, 17, 57, 25],
    [15, 47, 7, 39, 13, 45, 5, 37],
    [63, 31, 55, 23, 61, 29, 53, 21],
];

/// Expand a BGR555 palette to 8-bit [r, g, b] rows ((v<<3)|(v>>2), the same
/// expansion the renderer and web previews use).
fn expand(p: &[u16]) -> Vec<[i32; 3]> {
    p.iter()
        .map(|&c| {
            let e = |v: u16| {
                let v = v as i32;
                (v << 3) | (v >> 2)
            };
            [e(c & 0x1f), e((c >> 5) & 0x1f), e((c >> 10) & 0x1f)]
        })
        .collect()
}

/// Nearest palette row to an 8-bit color, same r2/g4/b3 luma weights as
/// `quantize::dist2`; ties break to the lowest index.
fn nearest8(pal: &[[i32; 3]], c: [i32; 3]) -> usize {
    let mut best = 0;
    let mut bd = i64::MAX;
    for (i, p) in pal.iter().enumerate() {
        let d = 2 * ((p[0] - c[0]) as i64).pow(2)
            + 4 * ((p[1] - c[1]) as i64).pow(2)
            + 3 * ((p[2] - c[2]) as i64).pow(2);
        if d < bd {
            bd = d;
            best = i;
        }
    }
    best
}

/// Remap an RGBA image into one `IndexTile` per 8x8 cell (row-major, cols x
/// rows, right/bottom padding transparent). `assignment[tile]` picks the
/// sub-palette; index 0 = transparent, index i+1 = palette color i. Pure and
/// deterministic for all modes (diffusion is fixed serpentine integer math).
pub fn remap_image(
    rgba: &[u8],
    width: usize,
    height: usize,
    palettes: &[Vec<u16>],
    assignment: &[u8],
    cols: usize,
    rows: usize,
    opts: &RemapOptions,
) -> Vec<IndexTile> {
    let pals8: Vec<Vec<[i32; 3]>> = palettes.iter().map(|p| expand(p)).collect();
    let empty: Vec<[i32; 3]> = Vec::new();
    let pal_at = |x: usize, y: usize| -> &Vec<[i32; 3]> {
        assignment
            .get((y / 8) * cols + x / 8)
            .and_then(|&a| pals8.get(a as usize))
            .unwrap_or(&empty)
    };
    let stride = cols * 8;
    let mut idx = vec![0u8; stride * rows * 8];
    let strength = opts.strength.min(100) as i32;

    match opts.dither {
        DitherMode::None | DitherMode::Bayer => {
            // Bayer offset: (2m - 63)/126 in [-0.5, 0.5), amplitude 0..=64 by
            // strength. amp 0 (or mode None) degenerates to flat nearest.
            let amp = match opts.dither {
                DitherMode::Bayer => strength * 64 / 100,
                _ => 0,
            };
            for y in 0..height {
                for x in 0..width {
                    let o = (y * width + x) * 4;
                    if rgba[o + 3] < opts.alpha_threshold {
                        continue;
                    }
                    let pal = pal_at(x, y);
                    if pal.is_empty() {
                        continue;
                    }
                    let off = (2 * BAYER8[y % 8][x % 8] - 63) * amp / 126;
                    let c = [
                        (rgba[o] as i32 + off).clamp(0, 255),
                        (rgba[o + 1] as i32 + off).clamp(0, 255),
                        (rgba[o + 2] as i32 + off).clamp(0, 255),
                    ];
                    idx[y * stride + x] = nearest8(pal, c) as u8 + 1;
                }
            }
        }
        DitherMode::Diffusion => {
            // Serpentine FS; error buffers accumulate in 1/16 units. Transparent
            // or palette-less pixels absorb (drop) incoming error so noise
            // never leaks across holes.
            let mut err_next: Vec<[i32; 3]> = vec![[0; 3]; width];
            for y in 0..height {
                let err_row = std::mem::replace(&mut err_next, vec![[0; 3]; width]);
                let ltr = y % 2 == 0;
                let mut carry = [0i32; 3]; // 1/16 units, into the next pixel
                let xs: Vec<usize> = if ltr {
                    (0..width).collect()
                } else {
                    (0..width).rev().collect()
                };
                for x in xs {
                    let o = (y * width + x) * 4;
                    let pal = pal_at(x, y);
                    if rgba[o + 3] < opts.alpha_threshold || pal.is_empty() {
                        carry = [0; 3];
                        continue;
                    }
                    let mut c = [0i32; 3];
                    for i in 0..3 {
                        c[i] = (rgba[o + i] as i32 + (err_row[x][i] + carry[i]) / 16).clamp(0, 255);
                    }
                    let k = nearest8(pal, c);
                    idx[y * stride + x] = k as u8 + 1;
                    let fwd = |x: usize| if ltr { x + 1 } else { x.wrapping_sub(1) };
                    let back = |x: usize| if ltr { x.wrapping_sub(1) } else { x + 1 };
                    for i in 0..3 {
                        let e = (c[i] - pal[k][i]) * strength / 100;
                        carry[i] = e * 7;
                        err_next[x][i] += e * 5;
                        if back(x) < width {
                            err_next[back(x)][i] += e * 3;
                        }
                        if fwd(x) < width {
                            err_next[fwd(x)][i] += e;
                        }
                    }
                }
            }
        }
    }

    (0..cols * rows)
        .map(|t| {
            let (tx, ty) = (t % cols, t / cols);
            std::array::from_fn(|i| idx[(ty * 8 + i / 8) * stride + tx * 8 + i % 8])
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 8x8 solid mid-gray between two palette grays.
    fn gray_rgba(v: u8) -> Vec<u8> {
        let mut out = Vec::new();
        for _ in 0..64 {
            out.extend_from_slice(&[v, v, v, 255]);
        }
        out
    }

    // 5-bit grays 12 (=8bit 99) and 16 (=8bit 132).
    const PAL: [u16; 2] = [(12 << 10) | (12 << 5) | 12, (16 << 10) | (16 << 5) | 16];

    #[test]
    fn flat_remap_matches_nearest_and_pads_transparent() {
        let rgba = gray_rgba(99);
        let tiles = remap_image(
            &rgba,
            8,
            8,
            &[PAL.to_vec()],
            &[0],
            1,
            1,
            &RemapOptions::default(),
        );
        assert_eq!(tiles.len(), 1);
        assert!(tiles[0].iter().all(|&i| i == 1)); // all snap to gray 12
    }

    #[test]
    fn bayer_mixes_between_neighbors_for_midpoint_color() {
        let rgba = gray_rgba(116); // midway between 99 and 132
        let opts = RemapOptions {
            dither: DitherMode::Bayer,
            strength: 100,
            ..Default::default()
        };
        let tiles = remap_image(&rgba, 8, 8, &[PAL.to_vec()], &[0], 1, 1, &opts);
        let ones = tiles[0].iter().filter(|&&i| i == 1).count();
        let twos = tiles[0].iter().filter(|&&i| i == 2).count();
        assert_eq!(ones + twos, 64);
        assert!(ones > 8 && twos > 8, "expected a mix, got {ones}/{twos}");
    }

    #[test]
    fn bayer_zero_strength_equals_flat() {
        let rgba = gray_rgba(116);
        let flat = remap_image(
            &rgba,
            8,
            8,
            &[PAL.to_vec()],
            &[0],
            1,
            1,
            &RemapOptions::default(),
        );
        let opts = RemapOptions {
            dither: DitherMode::Bayer,
            strength: 0,
            ..Default::default()
        };
        assert_eq!(
            remap_image(&rgba, 8, 8, &[PAL.to_vec()], &[0], 1, 1, &opts),
            flat
        );
    }

    #[test]
    fn diffusion_approximates_the_midpoint_ratio() {
        // 16x16 solid 116 over the same 2-gray palette: index counts should
        // land near 50/50 (the exact pattern is fixed by the serpentine order).
        let mut rgba = Vec::new();
        for _ in 0..256 {
            rgba.extend_from_slice(&[116, 116, 116, 255]);
        }
        let opts = RemapOptions {
            dither: DitherMode::Diffusion,
            strength: 100,
            ..Default::default()
        };
        let a = remap_image(&rgba, 16, 16, &[PAL.to_vec()], &[0, 0, 0, 0], 2, 2, &opts);
        let b = remap_image(&rgba, 16, 16, &[PAL.to_vec()], &[0, 0, 0, 0], 2, 2, &opts);
        assert_eq!(a, b); // deterministic
        let ones: usize = a.iter().flat_map(|t| t.iter()).filter(|&&i| i == 1).count();
        assert!((64..=192).contains(&ones), "ones = {ones}");
    }

    #[test]
    fn transparent_pixels_stay_index_zero_under_dither() {
        let mut rgba = gray_rgba(116);
        rgba[3] = 0; // pixel (0,0) transparent
        for mode in [DitherMode::Bayer, DitherMode::Diffusion] {
            let opts = RemapOptions {
                dither: mode,
                strength: 100,
                ..Default::default()
            };
            let tiles = remap_image(&rgba, 8, 8, &[PAL.to_vec()], &[0], 1, 1, &opts);
            assert_eq!(tiles[0][0], 0);
        }
    }

    #[test]
    fn alpha_threshold_is_honored() {
        let mut rgba = gray_rgba(116);
        rgba[3] = 100;
        let low = RemapOptions {
            alpha_threshold: 64,
            ..Default::default()
        };
        let high = RemapOptions {
            alpha_threshold: 200,
            ..Default::default()
        };
        assert_ne!(
            remap_image(&rgba, 8, 8, &[PAL.to_vec()], &[0], 1, 1, &low)[0][0],
            0
        );
        assert_eq!(
            remap_image(&rgba, 8, 8, &[PAL.to_vec()], &[0], 1, 1, &high)[0][0],
            0
        );
    }
}
