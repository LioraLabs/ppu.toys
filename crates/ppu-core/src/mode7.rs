//! Mode 7 affine rasterizer: per-scanline sampling of a whole-image source
//! through the m7 matrix (a,b,c,d) about center (cx,cy), with Q8 fixed-point
//! math internally (the DSL passes f32). Nearest-neighbor, wraps over the
//! source — the namesake receding "floor". Brightness/compositing live in E5.

use crate::memory::Source;
use crate::registers::{LineTableRow, Mode7};

/// Fixed-point fractional bits. `1.0 == 1 << FIX_SHIFT` (Q8 mirrors the SNES
/// 1.7.8 Mode 7 matrix format).
const FIX_SHIFT: u32 = 8;
const FIX_ONE: i64 = 1 << FIX_SHIFT; // 256

/// Convert an f32 (Lua-supplied) to Q8 fixed point, rounding to nearest.
#[inline]
fn to_fix8(v: f32) -> i64 {
    (v as f64 * FIX_ONE as f64).round() as i64
}

/// Map screen pixel (`x`, `y`) to integer source texel coordinates through the
/// Mode 7 affine transform. Coordinates may be negative or exceed the source;
/// the caller wraps. See the module/plan formula.
pub fn mode7_texel(m7: &Mode7, scroll_x: f32, scroll_y: f32, x: i32, y: i32) -> (i64, i64) {
    let a = to_fix8(m7.a);
    let b = to_fix8(m7.b);
    let c = to_fix8(m7.c);
    let d = to_fix8(m7.d);
    let cx = to_fix8(m7.cx);
    let cy = to_fix8(m7.cy);
    // Screen position offset by scroll (hofs/vofs) and the center, Q8.
    let px = ((x as i64) << FIX_SHIFT) + to_fix8(scroll_x) - cx;
    let py = ((y as i64) << FIX_SHIFT) + to_fix8(scroll_y) - cy;
    // Affine multiply: (Q8 * Q8) >> 8 = Q8, then re-add the center.
    let u = ((a * px) >> FIX_SHIFT) + ((b * py) >> FIX_SHIFT) + cx;
    let v = ((c * px) >> FIX_SHIFT) + ((d * py) >> FIX_SHIFT) + cy;
    // Drop fractional bits -> integer texel coordinates.
    (u >> FIX_SHIFT, v >> FIX_SHIFT)
}

/// Sample the Mode 7 floor for scanline `y` into `out` (length must be
/// `width * 4`). Uses `row.m7` and `row.bg[0]` scroll (DSL `bg[1]`). Wraps over
/// `src`, nearest-neighbor. An empty source produces a transparent scanline.
/// This is the seam the E5 compositor calls per scanline when `row.mode == 7`.
pub fn render_mode7_scanline(row: &LineTableRow, src: &Source, y: usize, out: &mut [u8]) {
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
pub fn render_mode7(lt: &crate::linetable::LineTable, src: &Source, width: usize, height: usize) -> Vec<u8> {
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

    #[test]
    fn identity_maps_screen_to_itself() {
        let m = Mode7::default(); // a=d=1, b=c=0, cx=cy=0
        assert_eq!(mode7_texel(&m, 0.0, 0.0, 0, 0), (0, 0));
        assert_eq!(mode7_texel(&m, 0.0, 0.0, 17, 42), (17, 42));
    }

    #[test]
    fn uniform_scale_multiplies_coords() {
        let m = Mode7 { a: 2.0, d: 2.0, ..Mode7::default() };
        assert_eq!(mode7_texel(&m, 0.0, 0.0, 10, 5), (20, 10));
    }

    #[test]
    fn scroll_offsets_the_sample() {
        let m = Mode7::default();
        assert_eq!(mode7_texel(&m, 4.0, 9.0, 1, 1), (5, 10));
    }

    #[test]
    fn center_is_the_fixed_point_of_scaling() {
        // With cx=128 and a=2, screen x=128 must map back to texel x=128.
        let m = Mode7 { a: 2.0, d: 2.0, cx: 128.0, cy: 0.0, ..Mode7::default() };
        let (tx, _) = mode7_texel(&m, 0.0, 0.0, 128, 0);
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
        let row = LineTableRow::default(); // identity m7, scroll 0
        let src = tiny_source();
        let mut out = [0u8; 8]; // width 2
        render_mode7_scanline(&row, &src, 0, &mut out);
        assert_eq!(out, [255, 0, 0, 255, 0, 255, 0, 255]); // source row 0
        render_mode7_scanline(&row, &src, 1, &mut out);
        assert_eq!(out, [0, 0, 255, 255, 255, 255, 255, 255]); // source row 1
    }

    #[test]
    fn sampling_wraps_past_source_width() {
        let row = LineTableRow::default();
        let src = tiny_source();
        let mut out = [0u8; 16]; // width 4 over a 2-wide source
        render_mode7_scanline(&row, &src, 0, &mut out);
        // x=2,3 wrap back to x=0,1.
        assert_eq!(&out[8..16], &out[0..8]);
    }

    #[test]
    fn empty_source_yields_transparent_scanline() {
        let row = LineTableRow::default();
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
