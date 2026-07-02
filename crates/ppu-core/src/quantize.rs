//! Canonical float->register quantization. The DSL surface is floats; the moment
//! a value crosses into a register it snaps to the authentic hardware repr HERE.
//! One quantization, shared by the LineTable build, the rasterizers, and the
//! inspector — so what renders and what the inspector shows can never diverge.

/// BG scroll offset (BGnHOFS/VOFS): whole pixels. Real hardware latches an
/// integer; sub-pixel tile scroll is not a hardware behavior. Round to nearest.
/// Not bit-masked in storage — the rasterizer wraps over the source image, so
/// the 10/13-bit register width is only applied for the inspector display.
/// ponytail: width mask deferred; whole-pixel rounding is the load-bearing part.
#[inline]
pub fn scroll_reg(v: f32) -> i16 {
    v.round() as i16
}

/// Mode 7 matrix entry a/b/c/d -> Q1.7.8 signed fixed point (`round(v*256)`).
#[inline]
pub fn m7_matrix(v: f32) -> i16 {
    (v * 256.0).round().clamp(i16::MIN as f32, i16::MAX as f32) as i16
}

/// Mode 7 center (M7X/M7Y): 13-bit signed whole-pixel coordinate. Round to nearest.
#[inline]
pub fn m7_center(v: f32) -> i16 {
    v.round() as i16
}

/// Sprite X: 9-bit signed screen coordinate (negatives = partially off-left).
/// ponytail: like `scroll_reg`, the bit-width mask is deferred — rounding is the
/// load-bearing part; the rasterizer clips off-screen x.
#[inline]
pub fn sprite_x(v: f32) -> i16 {
    v.round() as i16
}

/// Sprite Y: 8-bit screen coordinate; wraps mod 256 like OAM.
#[inline]
pub fn sprite_y(v: f32) -> u8 {
    (v.round() as i64).rem_euclid(256) as u8
}

/// INIDISP brightness: 4-bit (0..=15). Masks (wraps) like hardware.
#[inline]
pub fn brightness(v: u8) -> u8 {
    v & 0x0f
}

/// BGMODE: low 3 bits.
#[inline]
pub fn mode(v: u8) -> u8 {
    v & 0x07
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_rounds_to_whole_pixels() {
        assert_eq!(scroll_reg(10.0), 10);
        assert_eq!(scroll_reg(10.7), 11); // round to nearest, NOT floor
        assert_eq!(scroll_reg(-0.4), 0);
        assert_eq!(scroll_reg(-1.6), -2);
    }

    #[test]
    fn m7_matrix_is_q8_fixed_point() {
        assert_eq!(m7_matrix(1.0), 256); // 1.0 == 1<<8
        assert_eq!(m7_matrix(0.5), 128);
        assert_eq!(m7_matrix(-1.0), -256);
    }

    #[test]
    fn m7_center_rounds_to_whole_pixels() {
        assert_eq!(m7_center(128.0), 128);
        assert_eq!(m7_center(127.6), 128);
    }

    #[test]
    fn sprite_x_is_signed_sprite_y_wraps_u8() {
        assert_eq!(sprite_x(120.4), 120);
        assert_eq!(sprite_x(-8.0), -8); // 9-bit signed keeps negatives (off-left)
        assert_eq!(sprite_y(132.6), 133);
        assert_eq!(sprite_y(-1.0), 255); // 8-bit wraps
    }

    #[test]
    fn mode_and_brightness_mask_not_clamp() {
        assert_eq!(brightness(15), 15);
        assert_eq!(brightness(20), 4); // 20 & 0x0f, wraps (NOT clamp to 15)
        assert_eq!(mode(7), 7);
        assert_eq!(mode(9), 1); // 9 & 7
    }
}
