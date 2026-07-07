//! Mode-1 tile background rasterizer.
//!
//! The v1 direct-RGBA `Source` contract is gone (deleted by m4/memory): a BG
//! layer no longer names a whole-image asset. `render_bg_layer_scanline` is a
//! STUB — every layer renders transparent — until m4/bg-raster rewrites it as
//! the real tilemap/char/palette pipeline over byte-accurate VRAM. Brightness
//! is applied once by the E5 compositor; the per-layer primitive here returns
//! un-attenuated color.

use crate::memory::{unpack_rgb15, Memory};
use crate::registers::{RegBg, RegRow};

/// Attenuate one 8-bit channel by INIDISP brightness (0..=15). 15 is identity,
/// 0 is black. Integer + deterministic; values above 15 clamp to identity.
#[inline]
pub fn apply_brightness(channel: u8, brightness: u8) -> u8 {
    let b = brightness.min(15) as u16;
    ((channel as u16 * b) / 15) as u8
}

/// Attenuate an RGBA pixel's color channels by brightness; alpha untouched.
#[inline]
fn attenuate(mut px: [u8; 4], brightness: u8) -> [u8; 4] {
    px[0] = apply_brightness(px[0], brightness);
    px[1] = apply_brightness(px[1], brightness);
    px[2] = apply_brightness(px[2], brightness);
    px
}

/// Palette index of pixel (`fx`, `fy`) inside the 8x8 char whose bitplane
/// data starts at VRAM word `addr`. SNES layout: word `addr + fy` holds
/// plane 0 (low byte) and plane 1 (high byte) of row `fy`; 4bpp adds planes
/// 2/3 in word `addr + 8 + fy`. Bit 7 is the leftmost pixel. Fetches wrap
/// mod VRAM (0x8000 words).
fn char_pixel_index(mem: &Memory, addr: u16, bpp: u8, fx: u32, fy: u32) -> u8 {
    let bit = 7 - (fx & 7);
    let plane_pair = |w: u16| (((w as u8) >> bit) & 1) | ((((w >> 8) as u8) >> bit) & 1) << 1;
    let word = |off: u32| mem.vram[((addr as u32 + off) & 0x7fff) as usize];
    let mut index = plane_pair(word(fy));
    if bpp == 4 {
        index |= plane_pair(word(8 + fy)) << 2;
    }
    index
}

/// VRAM word address of the tilemap entry for tile column `tx`, row `ty`
/// (already wrapped to the layer's total tile extent). A tilemap is 1, 2, or
/// 4 32x32-entry screens of 0x400 words, arranged per the BGnSC screen size:
/// 0 = 32x32; 1 = 64x32 (screen 1 right); 2 = 32x64 (screen 1 below);
/// 3 = 64x64 (screens 0|1 over 2|3). Wraps mod VRAM.
fn map_entry_addr(map_base: u16, screen_size: u8, tx: u32, ty: u32) -> u16 {
    let screen = match screen_size {
        1 => tx / 32,
        2 => ty / 32,
        3 => (ty / 32) * 2 + tx / 32,
        _ => 0,
    };
    let off = screen * 0x400 + (ty % 32) * 32 + (tx % 32);
    ((map_base as u32 + off) & 0x7fff) as u16
}

/// Render one BG layer for scanline `y` into `width` pixel candidates.
/// `None` = transparent at that x (lower layer / backdrop shows through).
///
/// TODO(m4/bg-raster): STUB during the M4 substrate rewrite — the v1 direct-RGBA
/// `Source` path was deleted by m4/memory. m4/bg-raster rewrites this as the real
/// pipeline: tilemap fetch at `layer.map_base` (screen-size wrap) -> char
/// bitplane decode at `layer.char_base` -> CGRAM sub-palette lookup. Until
/// then every BG layer renders transparent.
pub fn render_bg_layer_scanline(
    layer: &RegBg,
    _mem: &Memory,
    _y: usize,
    width: usize,
) -> Vec<Option<[u8; 4]>> {
    let _ = layer.visible; // keeps the seam's inputs obvious for m4/bg-raster
    vec![None; width]
}

/// Standalone Mode-1 BG raster: backdrop (`cgram[0]`) + the four layers
/// (BG4..BG1, topmost wins) with INIDISP brightness applied once. Convenience
/// for the BG golden/unit tests ONLY — the E5 compositor composites layers
/// itself via `render_bg_layer_scanline` and applies brightness once globally,
/// so it never calls this (no double-attenuation).
pub fn render_bg_scanline(
    row: &RegRow,
    mem: &Memory,
    y: usize,
    width: usize,
) -> Vec<[u8; 4]> {
    let mut out = vec![unpack_rgb15(mem.cgram[0]); width];
    for layer in row.bg.iter().rev() {
        let line = render_bg_layer_scanline(layer, mem, y, width);
        for (slot, px) in out.iter_mut().zip(line) {
            if let Some(c) = px {
                *slot = c;
            }
        }
    }
    for px in out.iter_mut() {
        *px = attenuate(*px, row.brightness);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brightness_15_is_identity_0_is_black() {
        assert_eq!(apply_brightness(200, 15), 200);
        assert_eq!(apply_brightness(255, 15), 255);
        assert_eq!(apply_brightness(200, 0), 0);
    }

    #[test]
    fn brightness_scales_and_clamps() {
        // 200 * 8 / 15 = 106 (integer trunc)
        assert_eq!(apply_brightness(200, 8), 106);
        // brightness above 15 clamps to identity
        assert_eq!(apply_brightness(123, 99), 123);
    }

    use crate::memory::rgb15;
    use crate::registers::{LineTableRow, RegRow};

    // TODO(m4/bg-raster): the direct-RGBA sampling tests were deleted with the v1
    // `Source` model; m4/bg-raster adds VRAM-backed tilemap/char/palette tests.

    #[test]
    fn stub_layer_is_all_transparent() {
        let m = Memory::new();
        let row = RegRow::from(&LineTableRow::default());
        assert!(render_bg_layer_scanline(&row.bg[0], &m, 0, 8).iter().all(|p| p.is_none()));
    }

    #[test]
    fn composite_shows_backdrop_and_applies_brightness() {
        let mut m = Memory::new();
        m.cgram[0] = rgb15(200, 200, 200);
        let mut row = RegRow::from(&LineTableRow::default());
        row.brightness = 15;
        assert_eq!(render_bg_scanline(&row, &m, 0, 2)[0], unpack_rgb15(rgb15(200, 200, 200)));
        row.brightness = 0; // everything black
        assert_eq!(render_bg_scanline(&row, &m, 0, 2)[0], [0, 0, 0, 255]);
    }

    #[test]
    fn decodes_2bpp_bitplanes() {
        let mut m = Memory::new();
        // Row 0 of a 2bpp char at word 0x2000: pixels (0,0)=1, (1,0)=2, (2,0)=3.
        m.vram[0x2000] = (0b0110_0000 << 8) | 0b1010_0000;
        // Row 5: pixel (7,5) = 2 (plane 1, bit 0).
        m.vram[0x2005] = 0b0000_0001 << 8;
        assert_eq!(char_pixel_index(&m, 0x2000, 2, 0, 0), 1);
        assert_eq!(char_pixel_index(&m, 0x2000, 2, 1, 0), 2);
        assert_eq!(char_pixel_index(&m, 0x2000, 2, 2, 0), 3);
        assert_eq!(char_pixel_index(&m, 0x2000, 2, 3, 0), 0);
        assert_eq!(char_pixel_index(&m, 0x2000, 2, 7, 5), 2);
        assert_eq!(char_pixel_index(&m, 0x2000, 2, 7, 4), 0);
    }

    #[test]
    fn decodes_4bpp_bitplanes() {
        let mut m = Memory::new();
        // 4bpp char at 0x1000: planes 0/1 in words 0..8, planes 2/3 in words 8..16.
        // Pixel (0,3) set in planes 0+2 -> index 5; pixel (4,3) in planes 1+3 -> index 10.
        m.vram[0x1000 + 3] = (0b0000_1000 << 8) | 0b1000_0000;
        m.vram[0x1000 + 8 + 3] = (0b0000_1000 << 8) | 0b1000_0000;
        assert_eq!(char_pixel_index(&m, 0x1000, 4, 0, 3), 0b0101);
        assert_eq!(char_pixel_index(&m, 0x1000, 4, 4, 3), 0b1010);
        // Pixel (7,7) set in all four planes = 15.
        m.vram[0x1000 + 7] = 0x0101;
        m.vram[0x1000 + 8 + 7] = 0x0101;
        assert_eq!(char_pixel_index(&m, 0x1000, 4, 7, 7), 15);
        // Reading the same data as 2bpp ignores planes 2/3.
        assert_eq!(char_pixel_index(&m, 0x1000, 2, 0, 3), 1);
    }

    #[test]
    fn char_fetch_wraps_vram() {
        let mut m = Memory::new();
        // Row 1 of a char based at the last VRAM word wraps to 0x0000.
        m.vram[0x0000] = 0b1000_0000; // plane 0, bit 7
        assert_eq!(char_pixel_index(&m, 0x7fff, 2, 0, 1), 1);
    }

    #[test]
    fn map_entry_addr_walks_one_screen() {
        assert_eq!(map_entry_addr(0x0000, 0, 0, 0), 0x0000);
        assert_eq!(map_entry_addr(0x0000, 0, 31, 0), 31);
        assert_eq!(map_entry_addr(0x0000, 0, 0, 1), 32);
        assert_eq!(map_entry_addr(0x7c00, 0, 5, 3), 0x7c00 + 3 * 32 + 5);
    }

    #[test]
    fn map_entry_addr_selects_screens_per_size() {
        // 64x32: tile column 32+ lands in screen 1.
        assert_eq!(map_entry_addr(0x0000, 1, 32, 0), 0x0400);
        assert_eq!(map_entry_addr(0x0000, 1, 63, 31), 0x0400 + 31 * 32 + 31);
        // 32x64: tile row 32+ lands in screen 1.
        assert_eq!(map_entry_addr(0x0000, 2, 0, 32), 0x0400);
        // 64x64 quadrants: 0|1 over 2|3.
        assert_eq!(map_entry_addr(0x1000, 3, 32, 0), 0x1400);
        assert_eq!(map_entry_addr(0x1000, 3, 0, 32), 0x1800);
        assert_eq!(map_entry_addr(0x1000, 3, 32, 32), 0x1c00);
    }

    #[test]
    fn map_entry_addr_wraps_vram() {
        // map_base at the top of VRAM: screen 1 wraps around to word 0.
        assert_eq!(map_entry_addr(0x7c00, 1, 32, 0), 0x0000);
    }
}
