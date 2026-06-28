//! Mode-1 tile background rasterizer.
//!
//! Renders BG layers from their whole-image sources (`Bg::source`), auto-tiled
//! and wrapped, through the CGRAM palette, with INIDISP brightness attenuation.
//!
//! ## Paletted-source contract (v1)
//! The clean memory model stores each BG `Source` as decoded RGBA, but SNES BG
//! graphics are *indexed* and the DSL color-cycles via `cgram[]` (dusk-parallax).
//! So a BG source pixel is a CGRAM index, NOT a direct color:
//!   - `rgba[3] == 0` (alpha 0) -> transparent (lower layer / backdrop shows)
//!   - else `index = rgba[0]` (red channel, 0..=255)
//!   - `index == 0`             -> transparent (SNES color-0 convention)
//!   - else color = `unpack_rgb15(cgram[index])`, then brightness-attenuated.

use crate::memory::{unpack_rgb15, Memory};
use crate::registers::{Bg, LineTableRow};

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
}
