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

/// Every intermediate of the Mode-7 map -> char walk for screen pixel (x, y):
/// shared by the raster paths (via `mode7_raw_index`) and the Trace seam.
/// Out-of-field pixels: repeat 2 reports the wrapped coords with `index: 0`
/// (transparent); repeat 3 reports the tile-0 fill (`tile: 0`, `map_addr: 0`).
pub(crate) struct Mode7Sample {
    pub tx: u16,
    pub ty: u16,
    pub map_addr: u16,
    pub tile: u8,
    pub fx: u8,
    pub fy: u8,
    pub index: u8,
}

pub(crate) fn mode7_sample(row: &RegRow, mem: &Memory, y: usize, x: usize) -> Mode7Sample {
    let m7 = &row.m7;
    let bg = &row.bg[0];
    // Mode 7 mosaic is driven by the BG1 enable bit; block edge = size+1 (1 = off).
    // Snap the screen coordinate to the block's top-left before flip+affine, so the
    // whole block resolves to its top-left texel (same absolute-row-0 anchoring as the
    // tile BG path). Shared by the direct-CGRAM and EXTBG paths.
    let block = if row.mosaic_enable[0] {
        row.mosaic_size as i32 + 1
    } else {
        1
    };
    let my = y as i32 / block * block;
    let mx = x as i32 / block * block;
    let sy = if m7.flip_y { 255 - my } else { my };
    let sx = if m7.flip_x { 255 - mx } else { mx };
    let (u, v) = mode7_texel(m7, bg.scroll_x, bg.scroll_y, sx, sy);
    let in_field = (0..FIELD).contains(&u) && (0..FIELD).contains(&v);
    match (in_field, m7.repeat) {
        (false, 2) => {
            let (px, py) = (u.rem_euclid(FIELD), v.rem_euclid(FIELD));
            let (tx, ty) = (px >> 3, py >> 3);
            Mode7Sample {
                tx: tx as u16,
                ty: ty as u16,
                map_addr: (ty * 128 + tx) as u16,
                tile: (mem.vram[(ty * 128 + tx) as usize] & 0x00ff) as u8,
                fx: (px & 7) as u8,
                fy: (py & 7) as u8,
                index: 0, // out-of-field transparent
            }
        }
        (false, 3) => {
            let (fx, fy) = (u.rem_euclid(8), v.rem_euclid(8));
            Mode7Sample {
                tx: 0,
                ty: 0,
                map_addr: 0,
                tile: 0,
                fx: fx as u8,
                fy: fy as u8,
                index: (mem.vram[(fy * 8 + fx) as usize] >> 8) as u8,
            }
        }
        _ => {
            let (px, py) = (u.rem_euclid(FIELD), v.rem_euclid(FIELD));
            let (tx, ty) = (px >> 3, py >> 3);
            let (fx, fy) = (px & 7, py & 7);
            let tile = (mem.vram[(ty * 128 + tx) as usize] & 0x00ff) as u8;
            Mode7Sample {
                tx: tx as u16,
                ty: ty as u16,
                map_addr: (ty * 128 + tx) as u16,
                tile,
                fx: fx as u8,
                fy: fy as u8,
                index: (mem.vram[tile as usize * 64 + (fy * 8 + fx) as usize] >> 8) as u8,
            }
        }
    }
}

/// Raw 8bpp Mode-7 plane index (0..255) at screen pixel (`x`,`y`): applies
/// flip, the affine transform, and out-of-field `repeat` handling — the shared
/// sampling both the direct-CGRAM path and the EXTBG path build on.
fn mode7_raw_index(row: &RegRow, mem: &Memory, y: usize, x: usize) -> u8 {
    mode7_sample(row, mem, y, x).index
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
    for x in 0..width {
        let index = mode7_raw_index(row, mem, y, x);
        let oi = x * 4;
        out[oi..oi + 4].copy_from_slice(&if index == 0 {
            [0, 0, 0, 0] // palette index 0 = transparent
        } else if row.bg[0].direct_color {
            // Direct color (CGWSEL.0): 8bpp index -> BGR555, bypassing CGRAM.
            // No per-tile palette in Mode 7, so pal = 0. (EXTBG's own path,
            // render_mode7_scanline_px, applies direct color to its low 7 bits.)
            unpack_rgb15(crate::bg::direct_color_bgr555(index, 0))
        } else {
            unpack_rgb15(mem.cgram[index as usize])
        });
    }
}

/// One EXTBG Mode-7 pixel: resolved color + per-pixel priority (bit 7 of the
/// raw plane index). Analogous to `BgPixel`; `None` = transparent (color 0).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mode7Pixel {
    pub rgba: [u8; 4],
    pub prio: bool,
}

/// EXTBG (SETINI.6) sampling for scanline `y`: each raw plane index splits into
/// bit 7 = priority and low 7 bits (0..127) = CGRAM color. Color 0 = transparent
/// (`None`). Only used when `RegRow::extbg()`; the non-EXTBG path is unchanged.
pub fn render_mode7_scanline_px(
    row: &RegRow,
    mem: &Memory,
    y: usize,
    width: usize,
) -> Vec<Option<Mode7Pixel>> {
    let direct = row.bg[0].direct_color;
    (0..width)
        .map(|x| {
            let raw = mode7_raw_index(row, mem, y, x);
            let color = raw & 0x7f;
            (color != 0).then(|| Mode7Pixel {
                // Direct color (CGWSEL.0) expands the low-7-bit color to BGR555
                // (pal 0), bypassing CGRAM; otherwise it's a CGRAM index.
                rgba: if direct {
                    unpack_rgb15(crate::bg::direct_color_bgr555(color, 0))
                } else {
                    unpack_rgb15(mem.cgram[color as usize])
                },
                prio: raw & 0x80 != 0,
            })
        })
        .collect()
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
    fn mode7_direct_color_bypasses_cgram() {
        // pixel (0,0): map cell (0,0) -> tile 0; char high byte at tile 0 pixel (0,0) = index.
        let mut mem = Memory::new();
        set_map(&mut mem, 0, 0, 0);
        set_char(&mut mem, 0, 0, 0, 0xD3); // index 0xD3 = 0b11_010_011
                                           // If CGRAM were used it would read this; direct color must ignore it.
        mem.cgram[0xD3] = rgb15(9, 9, 9);
        let mut src = LineTableRow::default();
        src.mode = 7;
        src.cgwsel = 0x01; // direct color on
        let row = RegRow::from(&src);
        let mut out = [0u8; 4];
        render_mode7_scanline(&row, &mem, 0, &mut out);
        // pal = 0 (Mode 7 has no tilemap palette): r5=(3<<2)=12, g5=(2<<2)=8, b5=(3<<3)=24.
        assert_eq!(out, unpack_rgb15((24 << 10) | (8 << 5) | 12));
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

    #[test]
    fn mosaic_bg1_bit_pixelates_mode7_both_axes() {
        let mut mem = Memory::new();
        mem.cgram[5] = rgb15(255, 0, 0);
        set_map(&mut mem, 0, 0, 7);
        set_char(&mut mem, 7, 0, 0, 5); // ONLY plane pixel (0,0) is lit
        let mut src = LineTableRow::default();
        src.mosaic_size = 1; // 2x2 blocks
        src.mosaic_enable[0] = true; // BG1 bit drives Mode 7
        let row = RegRow::from(&src);
        // Row y=0: x=0 lit; x=1 replicates the block top-left (samples plane x=0).
        let mut out = [0u8; 4 * 4];
        render_mode7_scanline(&row, &mem, 0, &mut out);
        assert_eq!(&out[0..4], &unpack_rgb15(rgb15(255, 0, 0)));
        assert_eq!(&out[4..8], &unpack_rgb15(rgb15(255, 0, 0))); // horizontal replicate
        // Row y=1 snaps up to sample plane row 0 -> same lit pixel replicated down.
        render_mode7_scanline(&row, &mem, 1, &mut out);
        assert_eq!(&out[0..4], &unpack_rgb15(rgb15(255, 0, 0)));
    }

    #[test]
    fn mode7_mosaic_ignores_non_bg1_enable_bits() {
        let mut mem = Memory::new();
        mem.cgram[5] = rgb15(255, 0, 0);
        set_map(&mut mem, 0, 0, 7);
        set_char(&mut mem, 7, 0, 0, 5); // only plane pixel (0,0) lit
        let mut src = LineTableRow::default();
        src.mosaic_size = 1;
        src.mosaic_enable = [false, true, true, true]; // BG2-4 set, BG1 clear
        let row = RegRow::from(&src);
        let mut out = [0u8; 4 * 4];
        render_mode7_scanline(&row, &mem, 0, &mut out);
        // BG1 bit clear -> no mosaic -> x=1 samples plane x=1 (empty) = transparent.
        assert_eq!(&out[0..4], &unpack_rgb15(rgb15(255, 0, 0)));
        assert_eq!(&out[4..8], &[0, 0, 0, 0]);
    }

    #[test]
    fn non_extbg_scanline_unchanged_after_raw_index_extraction() {
        // Guard: the raw-index extraction MUST NOT change non-EXTBG output.
        // 8bpp index 200 resolves through the full 256-color CGRAM as before.
        let mut mem = Memory::new();
        mem.cgram[200] = rgb15(1, 2, 3);
        set_map(&mut mem, 0, 0, 7);
        set_char(&mut mem, 7, 0, 0, 200);
        let row = RegRow::from(&LineTableRow::default());
        let mut out = [0u8; 4];
        render_mode7_scanline(&row, &mem, 0, &mut out);
        assert_eq!(&out[0..4], &unpack_rgb15(rgb15(1, 2, 3)));
    }

    #[test]
    fn extbg_px_splits_bit7_priority_from_low7_color() {
        let mut mem = Memory::new();
        mem.cgram[5] = rgb15(255, 0, 0);
        set_map(&mut mem, 0, 0, 7);
        set_char(&mut mem, 7, 0, 0, 0x85); // bit7 set (high) + color 5
        set_char(&mut mem, 7, 1, 0, 0x03); // bit7 clear (low) + color 3
        set_char(&mut mem, 7, 2, 0, 0x80); // bit7 set but color 0 -> transparent
        mem.cgram[3] = rgb15(0, 255, 0);
        let row = RegRow::from(&LineTableRow::default());
        let px = render_mode7_scanline_px(&row, &mem, 0, 8);
        let hi = px[0].expect("x0 opaque");
        assert!(hi.prio); // bit7 -> high priority
        assert_eq!(hi.rgba, unpack_rgb15(rgb15(255, 0, 0))); // color = low 7 bits = 5
        let lo = px[1].expect("x1 opaque");
        assert!(!lo.prio); // bit7 clear -> low priority
        assert_eq!(lo.rgba, unpack_rgb15(rgb15(0, 255, 0))); // color 3
        assert!(px[2].is_none()); // masked color 0 is transparent even with bit7
    }
}
