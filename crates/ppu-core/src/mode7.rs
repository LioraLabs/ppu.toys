//! Mode 7 affine rasterizer: per-scanline sampling of a whole-image source
//! through the m7 matrix (a,b,c,d) about center (cx,cy), with Q8 fixed-point
//! math internally (the DSL passes f32). Nearest-neighbor, wraps over the
//! source — the namesake receding "floor". Brightness/compositing live in E5.

use crate::linetable::LineTable;
use crate::memory::Source;
use crate::registers::{RegM7, RegRow};

/// Fixed-point fractional bits. `1.0 == 1 << FIX_SHIFT` (Q8 mirrors the SNES
/// 1.7.8 Mode 7 matrix format).
const FIX_SHIFT: u32 = 8;

/// Map screen pixel (`x`,`y`) to integer source texel coords through the Mode 7
/// affine transform. a/b/c/d arrive as Q8 fixed point; cx/cy and scroll are
/// whole pixels (shifted into Q8 here). Caller wraps out-of-range texels.
pub fn mode7_texel(m7: &RegM7, scroll_x: i16, scroll_y: i16, x: i32, y: i32) -> (i64, i64) {
    let (a, b, c, d) = (m7.a as i64, m7.b as i64, m7.c as i64, m7.d as i64);
    let cx = (m7.cx as i64) << FIX_SHIFT;
    let cy = (m7.cy as i64) << FIX_SHIFT;
    let px = ((x as i64) << FIX_SHIFT) + ((scroll_x as i64) << FIX_SHIFT) - cx;
    let py = ((y as i64) << FIX_SHIFT) + ((scroll_y as i64) << FIX_SHIFT) - cy;
    let u = ((a * px) >> FIX_SHIFT) + ((b * py) >> FIX_SHIFT) + cx;
    let v = ((c * px) >> FIX_SHIFT) + ((d * py) >> FIX_SHIFT) + cy;
    (u >> FIX_SHIFT, v >> FIX_SHIFT)
}

/// Sample the Mode 7 floor for scanline `y` into `out` (length must be
/// `width * 4`). Uses `row.m7` and `row.bg[0]` scroll (DSL `bg[1]`). Wraps over
/// `src`, nearest-neighbor. An empty source produces a transparent scanline.
/// This is the seam the E5 compositor calls per scanline when `row.mode == 7`.
pub fn render_mode7_scanline(row: &RegRow, src: &Source, y: usize, out: &mut [u8]) {
    let width = out.len() / 4;
    if src.width == 0 || src.height == 0 || src.rgba.is_empty() {
        out.iter_mut().for_each(|b| *b = 0);
        return;
    }
    let bg = &row.bg[0];
    let sw = src.width as i64;
    let sh = src.height as i64;
    for x in 0..width {
        let (tx, ty) = mode7_texel(&row.m7, bg.scroll_x, bg.scroll_y, x as i32, y as i32);
        let sx = tx.rem_euclid(sw) as usize;
        let sy = ty.rem_euclid(sh) as usize;
        let si = (sy * src.width as usize + sx) * 4;
        let oi = x * 4;
        out[oi..oi + 4].copy_from_slice(&src.rgba[si..si + 4]);
    }
}

/// Render every scanline of a resolved line table as Mode 7 over `src` into a
/// fresh `width * height * 4` RGBA buffer. Convenience for the golden test and a
/// pure full-frame floor; the compositor composes scanlines itself.
pub fn render_mode7(lt: &LineTable, src: &Source, width: usize, height: usize) -> Vec<u8> {
    let mut fb = vec![0u8; width * height * 4];
    for y in 0..height {
        let off = y * width * 4;
        render_mode7_scanline(&lt.rows[y], src, y, &mut fb[off..off + width * 4]);
    }
    fb
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registers::{LineTableRow, Mode7};

    #[test]
    fn identity_maps_screen_to_itself() {
        let m = RegM7::from(&Mode7::default()); // a=d=1, b=c=0, cx=cy=0
        assert_eq!(mode7_texel(&m, 0, 0, 0, 0), (0, 0));
        assert_eq!(mode7_texel(&m, 0, 0, 17, 42), (17, 42));
    }

    #[test]
    fn uniform_scale_multiplies_coords() {
        let m = RegM7::from(&Mode7 { a: 2.0, d: 2.0, ..Mode7::default() });
        assert_eq!(mode7_texel(&m, 0, 0, 10, 5), (20, 10));
    }

    #[test]
    fn scroll_offsets_the_sample() {
        let m = RegM7::from(&Mode7::default());
        assert_eq!(mode7_texel(&m, 4, 9, 1, 1), (5, 10));
    }

    #[test]
    fn center_is_the_fixed_point_of_scaling() {
        // With cx=128 and a=2, screen x=128 must map back to texel x=128.
        let m = RegM7::from(&Mode7 { a: 2.0, d: 2.0, cx: 128.0, cy: 0.0, ..Mode7::default() });
        let (tx, _) = mode7_texel(&m, 0, 0, 128, 0);
        assert_eq!(tx, 128);
    }

    use crate::linetable::LineTableBuilder;

    // 2x2 source, four distinct colors: (0,0)=red (1,0)=green (0,1)=blue (1,1)=white.
    fn tiny_source() -> Source {
        Source {
            width: 2,
            height: 2,
            rgba: vec![
                255, 0, 0, 255,   0, 255, 0, 255,
                0, 0, 255, 255,   255, 255, 255, 255,
            ],
        }
    }

    #[test]
    fn identity_scanline_copies_source_row() {
        let row = RegRow::from(&LineTableRow::default()); // identity m7, scroll 0
        let src = tiny_source();
        let mut out = [0u8; 8]; // width 2
        render_mode7_scanline(&row, &src, 0, &mut out);
        assert_eq!(out, [255, 0, 0, 255, 0, 255, 0, 255]); // source row 0
        render_mode7_scanline(&row, &src, 1, &mut out);
        assert_eq!(out, [0, 0, 255, 255, 255, 255, 255, 255]); // source row 1
    }

    #[test]
    fn sampling_wraps_past_source_width() {
        let row = RegRow::from(&LineTableRow::default());
        let src = tiny_source();
        let mut out = [0u8; 16]; // width 4 over a 2-wide source
        render_mode7_scanline(&row, &src, 0, &mut out);
        // x=2,3 wrap back to x=0,1.
        assert_eq!(&out[8..16], &out[0..8]);
    }

    #[test]
    fn sampling_wraps_negative_texels() {
        // Negative scroll drives the texel x to -1; `rem_euclid` must wrap it to
        // the last column (1), where a plain `%` would mishandle the sign. This
        // is the case the floor effect actually hits.
        let mut row = RegRow::from(&LineTableRow::default());
        row.bg[0].scroll_x = -1;
        let src = tiny_source();
        let mut out = [0u8; 8]; // width 2
        render_mode7_scanline(&row, &src, 0, &mut out);
        assert_eq!(&out[0..4], &[0, 255, 0, 255]); // screen x=0 -> texel -1 wraps to col 1 (green)
        assert_eq!(&out[4..8], &[255, 0, 0, 255]); // screen x=1 -> texel 0 (red)
    }

    #[test]
    fn empty_source_yields_transparent_scanline() {
        let row = RegRow::from(&LineTableRow::default());
        let src = Source { width: 0, height: 0, rgba: vec![] };
        let mut out = [9u8; 8];
        render_mode7_scanline(&row, &src, 0, &mut out);
        assert_eq!(out, [0u8; 8]);
    }

    #[test]
    fn render_mode7_fills_whole_frame() {
        let b = LineTableBuilder::new(LineTableRow::default());
        let lt = b.build(2);
        let fb = render_mode7(&lt, &tiny_source(), 2, 2);
        assert_eq!(fb.len(), 2 * 2 * 4);
        assert_eq!(&fb[0..4], &[255, 0, 0, 255]); // (0,0) red under identity
    }
}
