//! Mode 7 affine rasterizer: per-scanline sampling through the m7 matrix
//! (a,b,c,d) about center (cx,cy), with Q8 fixed-point math internally (the
//! DSL passes f32). Nearest-neighbor, wraps over the source — the namesake
//! receding "floor". Brightness/compositing live in E5.
//!
//! `render_mode7_scanline`/`render_mode7` are STUBs (tag m4/mode7): the v1
//! direct-RGBA `Source` path was deleted by m4/memory. `mode7_texel` (the
//! affine math) is unaffected and stays fully tested.

use crate::linetable::LineTable;
use crate::memory::Memory;
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

/// Sample the Mode 7 floor for scanline `y` into `out` (length `width * 4`).
///
/// TODO(m4/mode7): STUB during the M4 substrate rewrite — the v1 direct-RGBA
/// `Source` path was deleted by m4/memory. m4/mode7 rewrites this to sample the
/// byte-interleaved VRAM (low byte = 128x128 tilemap, high byte = linear 8bpp
/// char) through `mode7_texel`, honoring `row.m7.repeat`/`flip_x`/`flip_y`.
/// Until then Mode 7 renders transparent.
pub fn render_mode7_scanline(_row: &RegRow, _mem: &Memory, _y: usize, out: &mut [u8]) {
    out.iter_mut().for_each(|b| *b = 0);
}

/// Render every scanline of a resolved line table as Mode 7 over `mem` into a
/// fresh `width * height * 4` RGBA buffer. TODO(m4/mode7): stub, see above.
pub fn render_mode7(lt: &LineTable, mem: &Memory, width: usize, height: usize) -> Vec<u8> {
    let mut fb = vec![0u8; width * height * 4];
    for y in 0..height {
        let off = y * width * 4;
        render_mode7_scanline(&lt.rows[y], mem, y, &mut fb[off..off + width * 4]);
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

    // TODO(m4/mode7): scanline sampling tests return with the interleaved-VRAM
    // rasterizer; only the affine math is testable against the stub.

    #[test]
    fn stub_scanline_is_transparent() {
        let row = RegRow::from(&LineTableRow::default());
        let mut out = [9u8; 8];
        render_mode7_scanline(&row, &Memory::new(), 0, &mut out);
        assert_eq!(out, [0u8; 8]);
    }
}
