//! Mode 7 affine rasterizer: per-scanline sampling of a whole-image source
//! through the m7 matrix (a,b,c,d) about center (cx,cy), with Q8 fixed-point
//! math internally (the DSL passes f32). Nearest-neighbor, wraps over the
//! source — the namesake receding "floor". Brightness/compositing live in E5.

use crate::registers::Mode7;

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
}
