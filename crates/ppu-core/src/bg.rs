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
}
