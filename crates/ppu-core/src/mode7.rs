//! Mode 7 affine rasterizer over the byte-interleaved Mode 7 VRAM: per-scanline
//! sampling through the m7 matrix (a,b,c,d) about center (cx,cy), Q8 fixed-point
//! internally (the DSL passes f32) — the namesake receding "floor".
//!
//! VRAM layout (fixed at word 0; no map/char base registers apply): each word's
//! LOW byte is one 128x128 tilemap cell (tile# 0-255, no attributes) and its
//! HIGH byte is linear 8bpp char data (256 tiles x 64 bytes) — one word read =
//! one map byte + one char byte, the hardware bandwidth trick. Palette index 0
//! is transparent; the rest resolve through the flat 256-color CGRAM. M7SEL:
//! `repeat` = out-of-field behavior (0/1 wrap, 2 transparent, 3 tile-0 fill),
//! `flip_x`/`flip_y` mirror the screen. Brightness/compositing live in E5.

use crate::linetable::LineTable;
use crate::memory::{unpack_rgb15, Memory};
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

/// Mode 7 plane size in pixels: 128 tiles x 8 px.
const FIELD: i64 = 1024;

/// 8bpp palette index at plane pixel (`px`,`py`), both already wrapped into
/// 0..1024: map LOW byte -> tile#, char HIGH byte -> pixel (linear 8bpp).
fn field_pixel(mem: &Memory, px: i64, py: i64) -> u8 {
    let (tx, ty) = (px >> 3, py >> 3);
    let (fx, fy) = (px & 7, py & 7);
    let tile = (mem.vram[(ty * 128 + tx) as usize] & 0x00ff) as i64;
    (mem.vram[(tile * 64 + fy * 8 + fx) as usize] >> 8) as u8
}

/// Sample the Mode 7 floor for scanline `y` into `out` (length `width * 4`):
/// affine-map each screen pixel through `mode7_texel` (scroll from
/// `row.bg[0]`, DSL `bg[1]`), then map LOW byte -> char HIGH byte -> CGRAM.
/// Palette index 0 is transparent (alpha 0). Out-of-field pixels follow
/// `m7.repeat` (0/1 wrap, 2 transparent, 3 tile-0 fill); `flip_x`/`flip_y`
/// mirror the screen axes before the transform. Un-attenuated; brightness is
/// the compositor's job.
pub fn render_mode7_scanline(row: &RegRow, mem: &Memory, y: usize, out: &mut [u8]) {
    let width = out.len() / 4;
    let m7 = &row.m7;
    let bg = &row.bg[0];
    let sy = if m7.flip_y { 255 - y as i32 } else { y as i32 };
    for x in 0..width {
        let sx = if m7.flip_x { 255 - x as i32 } else { x as i32 };
        let (u, v) = mode7_texel(m7, bg.scroll_x, bg.scroll_y, sx, sy);
        let in_field = (0..FIELD).contains(&u) && (0..FIELD).contains(&v);
        let index = match (in_field, m7.repeat) {
            (false, 2) => 0, // out-of-field transparent
            (false, 3) => {
                // out-of-field tile-0 fill: tile 0's char at the sub-tile pos
                let (fx, fy) = (u.rem_euclid(8), v.rem_euclid(8));
                (mem.vram[(fy * 8 + fx) as usize] >> 8) as u8
            }
            _ => field_pixel(mem, u.rem_euclid(FIELD), v.rem_euclid(FIELD)),
        };
        let oi = x * 4;
        out[oi..oi + 4].copy_from_slice(&if index == 0 {
            [0, 0, 0, 0] // palette index 0 = transparent
        } else {
            unpack_rgb15(mem.cgram[index as usize])
        });
    }
}

/// Render every scanline of a resolved line table as Mode 7 over `mem` into a
/// fresh `width * height * 4` RGBA buffer.
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
        let m = RegM7::from(&Mode7 {
            a: 2.0,
            d: 2.0,
            ..Mode7::default()
        });
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
        let m = RegM7::from(&Mode7 {
            a: 2.0,
            d: 2.0,
            cx: 128.0,
            cy: 0.0,
            ..Mode7::default()
        });
        let (tx, _) = mode7_texel(&m, 0, 0, 128, 0);
        assert_eq!(tx, 128);
    }

    use crate::linetable::LineTableBuilder;
    use crate::memory::{rgb15, unpack_rgb15, Memory};

    /// Poke the LOW byte of the map word for tilemap cell (tx, ty).
    fn set_map(mem: &mut Memory, tx: usize, ty: usize, tile: u8) {
        let i = ty * 128 + tx;
        mem.vram[i] = (mem.vram[i] & 0xff00) | tile as u16;
    }

    /// Poke the HIGH byte (char lane) of tile `tile`'s pixel (fx, fy).
    fn set_char(mem: &mut Memory, tile: usize, fx: usize, fy: usize, px: u8) {
        let i = tile * 64 + fy * 8 + fx;
        mem.vram[i] = (mem.vram[i] & 0x00ff) | ((px as u16) << 8);
    }

    #[test]
    fn identity_reads_map_then_char_then_cgram() {
        let mut mem = Memory::new();
        mem.cgram[5] = rgb15(255, 0, 0);
        set_map(&mut mem, 0, 0, 7); // map cell (0,0) -> tile 7
        set_char(&mut mem, 7, 3, 0, 5); // tile 7 pixel (3,0) -> palette index 5
        let row = RegRow::from(&LineTableRow::default()); // identity m7
        let mut out = [9u8; 8 * 4];
        render_mode7_scanline(&row, &mem, 0, &mut out);
        assert_eq!(&out[12..16], &unpack_rgb15(rgb15(255, 0, 0))); // x=3 painted
        assert_eq!(&out[0..4], &[0, 0, 0, 0]); // x=0: char byte 0 = transparent
    }

    #[test]
    fn one_word_carries_both_lanes() {
        // vram[0] serves map cell (0,0) in its LOW byte AND tile 0's pixel
        // (0,0) in its HIGH byte — the interleave.
        let mut mem = Memory::new();
        mem.vram[0] = (9u16 << 8) | 7; // char lane: index 9; map lane: tile 7
        mem.cgram[9] = rgb15(0, 255, 0);
        let row = RegRow::from(&LineTableRow::default());
        let mut out = [0u8; 16 * 4];
        render_mode7_scanline(&row, &mem, 0, &mut out);
        // Screen x=8 -> map cell (1,0), still tile 0 (default) -> tile 0 char
        // pixel (0,0) = the HIGH byte of vram[0] = index 9.
        assert_eq!(&out[8 * 4..8 * 4 + 4], &unpack_rgb15(rgb15(0, 255, 0)));
        // Screen x=0 -> tile 7, whose char data is all zero -> transparent
        // (the map lane of vram[0] did NOT leak into the char lane).
        assert_eq!(&out[0..4], &[0, 0, 0, 0]);
    }

    #[test]
    fn map_rows_stride_128_words() {
        let mut mem = Memory::new();
        mem.cgram[3] = rgb15(0, 0, 255);
        set_map(&mut mem, 0, 1, 2); // second map row -> word 128
        set_char(&mut mem, 2, 0, 0, 3);
        let row = RegRow::from(&LineTableRow::default());
        let mut out = [0u8; 4];
        render_mode7_scanline(&row, &mem, 8, &mut out); // screen y=8 -> ty=1, fy=0
        assert_eq!(&out[0..4], &unpack_rgb15(rgb15(0, 0, 255)));
    }

    #[test]
    fn index_zero_is_transparent_even_if_cgram0_is_colored() {
        let mut mem = Memory::new();
        mem.cgram[0] = rgb15(255, 255, 255); // backdrop color must NOT bleed in
        let row = RegRow::from(&LineTableRow::default());
        let mut out = [9u8; 8];
        render_mode7_scanline(&row, &mem, 0, &mut out);
        assert_eq!(out, [0u8; 8]);
    }

    #[test]
    fn render_mode7_fills_whole_frame() {
        let mut mem = Memory::new();
        mem.cgram[2] = rgb15(255, 255, 255);
        set_map(&mut mem, 0, 0, 1);
        set_char(&mut mem, 1, 0, 0, 2); // only plane pixel (0,0) is set
        let lt = LineTableBuilder::new(LineTableRow::default()).build(2);
        let fb = render_mode7(&lt, &mem, 2, 2);
        assert_eq!(fb.len(), 2 * 2 * 4);
        assert_eq!(&fb[0..4], &unpack_rgb15(rgb15(255, 255, 255))); // (0,0)
        assert_eq!(&fb[4..8], &[0, 0, 0, 0]); // (1,0) unset char -> transparent
    }

    /// Row with identity matrix and scroll_x = -1: screen x=0 -> plane u=-1
    /// (out of field), screen x=1 -> plane u=0 (in field).
    fn row_scrolled_left(repeat: u8) -> RegRow {
        let mut src = LineTableRow::default();
        src.bg[0].scroll_x = -1.0;
        src.m7.repeat = repeat;
        RegRow::from(&src)
    }

    #[test]
    fn repeat_0_wraps_out_of_field_to_far_column() {
        let mut mem = Memory::new();
        mem.cgram[1] = rgb15(255, 0, 0);
        set_map(&mut mem, 127, 0, 1); // rightmost map column
        set_char(&mut mem, 1, 7, 0, 1); // plane pixel (1023, 0)
        let mut out = [0u8; 8];
        render_mode7_scanline(&row_scrolled_left(0), &mem, 0, &mut out);
        assert_eq!(&out[0..4], &unpack_rgb15(rgb15(255, 0, 0))); // u=-1 wraps to 1023
        assert_eq!(&out[4..8], &[0, 0, 0, 0]); // u=0 in-field, empty
    }

    #[test]
    fn repeat_2_is_transparent_out_of_field() {
        let mut mem = Memory::new();
        mem.cgram[1] = rgb15(255, 0, 0);
        set_map(&mut mem, 127, 0, 1);
        set_char(&mut mem, 1, 7, 0, 1); // would show under wrap
        set_map(&mut mem, 0, 0, 1);
        set_char(&mut mem, 1, 0, 0, 1); // in-field control at plane (0,0)
        let mut out = [0u8; 8];
        render_mode7_scanline(&row_scrolled_left(2), &mem, 0, &mut out);
        assert_eq!(&out[0..4], &[0, 0, 0, 0]); // u=-1: transparent, no wrap
        assert_eq!(&out[4..8], &unpack_rgb15(rgb15(255, 0, 0))); // u=0 in-field normal
    }

    #[test]
    fn repeat_3_fills_out_of_field_with_tile_0() {
        let mut mem = Memory::new();
        mem.cgram[4] = rgb15(0, 255, 0);
        mem.cgram[1] = rgb15(255, 0, 0);
        set_char(&mut mem, 0, 7, 0, 4); // tile 0 pixel (7,0): u=-1 -> fx=7
        set_map(&mut mem, 0, 0, 1); // in-field control: map (0,0) -> tile 1
        set_char(&mut mem, 1, 0, 0, 1);
        let mut out = [0u8; 8];
        render_mode7_scanline(&row_scrolled_left(3), &mem, 0, &mut out);
        assert_eq!(&out[0..4], &unpack_rgb15(rgb15(0, 255, 0))); // tile-0 fill
                                                                 // u=0 is in-field: normal sampling (tile 1), NOT tile-0 fill.
        assert_eq!(&out[4..8], &unpack_rgb15(rgb15(255, 0, 0)));
    }

    #[test]
    fn repeat_1_also_wraps() {
        // Hardware treats M7SEL screen-over 0 and 1 identically (wrap).
        let mut mem = Memory::new();
        mem.cgram[1] = rgb15(255, 0, 0);
        set_map(&mut mem, 127, 0, 1);
        set_char(&mut mem, 1, 7, 0, 1);
        let mut out = [0u8; 4];
        render_mode7_scanline(&row_scrolled_left(1), &mem, 0, &mut out);
        assert_eq!(&out[0..4], &unpack_rgb15(rgb15(255, 0, 0)));
    }

    #[test]
    fn flip_x_mirrors_the_screen_horizontally() {
        let mut mem = Memory::new();
        mem.cgram[1] = rgb15(255, 0, 0);
        set_map(&mut mem, 31, 0, 1); // plane pixel (255, 0): cell (31,0), fx=7
        set_char(&mut mem, 1, 7, 0, 1);
        let mut src = LineTableRow::default();
        src.m7.flip_x = true;
        let row = RegRow::from(&src);
        let mut out = [0u8; 8];
        render_mode7_scanline(&row, &mem, 0, &mut out);
        // Screen x=0 samples plane x = 255 - 0 = 255.
        assert_eq!(&out[0..4], &unpack_rgb15(rgb15(255, 0, 0)));
        // Screen x=1 samples plane x = 254 (unset).
        assert_eq!(&out[4..8], &[0, 0, 0, 0]);
    }

    #[test]
    fn flip_y_mirrors_the_screen_vertically() {
        let mut mem = Memory::new();
        mem.cgram[1] = rgb15(0, 0, 255);
        set_map(&mut mem, 0, 31, 1); // plane pixel (0, 255): cell (0,31), fy=7
        set_char(&mut mem, 1, 0, 7, 1);
        let mut src = LineTableRow::default();
        src.m7.flip_y = true;
        let row = RegRow::from(&src);
        let mut out = [0u8; 4];
        // Screen y=0 samples plane y = 255 - 0 = 255.
        render_mode7_scanline(&row, &mem, 0, &mut out);
        assert_eq!(&out[0..4], &unpack_rgb15(rgb15(0, 0, 255)));
        // Screen y=1 samples plane y = 254 (unset).
        render_mode7_scanline(&row, &mem, 1, &mut out);
        assert_eq!(&out[0..4], &[0, 0, 0, 0]);
    }

    #[test]
    fn flip_applies_before_the_affine_transform() {
        // Hardware XORs the SCREEN coordinate, so with a=2 (scale) and flip_x,
        // screen x=0 must sample plane u = 2 * 255 = 510, not 255.
        let mut mem = Memory::new();
        mem.cgram[1] = rgb15(255, 255, 255);
        set_map(&mut mem, 63, 0, 1); // plane pixel (510, 0): cell (63,0), fx=6
        set_char(&mut mem, 1, 6, 0, 1);
        let mut src = LineTableRow::default();
        src.m7.a = 2.0;
        src.m7.flip_x = true;
        let row = RegRow::from(&src);
        let mut out = [0u8; 4];
        render_mode7_scanline(&row, &mem, 0, &mut out);
        assert_eq!(&out[0..4], &unpack_rgb15(rgb15(255, 255, 255)));
    }

    #[test]
    fn out_of_field_applies_to_the_v_axis_too() {
        // scroll_y = -1: screen y=0 -> plane v=-1 (out of field). Under
        // repeat=2 that pixel is transparent even though u=0 is in field.
        let mut mem = Memory::new();
        mem.cgram[1] = rgb15(255, 0, 0);
        set_map(&mut mem, 0, 127, 1); // plane (0,1023): would show under wrap
        set_char(&mut mem, 1, 0, 7, 1);
        let mut src = LineTableRow::default();
        src.bg[0].scroll_y = -1.0;
        src.m7.repeat = 2;
        let row = RegRow::from(&src);
        let mut out = [0u8; 4];
        render_mode7_scanline(&row, &mem, 0, &mut out);
        assert_eq!(&out[0..4], &[0, 0, 0, 0]); // v=-1: transparent
        src.m7.repeat = 0;
        let row = RegRow::from(&src);
        render_mode7_scanline(&row, &mem, 0, &mut out);
        assert_eq!(&out[0..4], &unpack_rgb15(rgb15(255, 0, 0))); // v=-1 wraps to 1023
    }
}
