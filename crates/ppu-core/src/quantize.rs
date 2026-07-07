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

/// BG tile size (BGMODE bits 4-7, one bit per layer). The DSL authors the pixel
/// edge (8 or 16); anything >= 16 snaps to 16x16, else 8x8. Stored as the pixel
/// edge — the BGMODE bit is derived as `tile_size == 16` at display time.
#[inline]
pub fn bg_tile_size(v: u8) -> u8 {
    if v >= 16 {
        16
    } else {
        8
    }
}

/// BGnSC screen size (bits 0-1): 0=32x32, 1=64x32, 2=32x64, 3=64x64 tiles.
/// Masks (wraps) like hardware.
#[inline]
pub fn bg_screen_size(v: u8) -> u8 {
    v & 0x03
}

/// BGnSC tilemap base (bits 2-7): a VRAM word address snapped DOWN to the
/// 0x400-word (2KB) hardware step and wrapped into VRAM (mod 0x8000 words),
/// exactly where a hardware fetch would land. Stored as the snapped EFFECTIVE
/// word address; the register bit-field is `map_base >> 10` (the raw 6-bit
/// field's aliasing top bit is not preserved — a v1 inspector simplification,
/// same spirit as `scroll_reg`'s deferred width mask).
#[inline]
pub fn bg_map_base(v: u32) -> u16 {
    (((v >> 10) & 0x1f) << 10) as u16
}

/// BG12NBA/BG34NBA char base (4 bits per layer): a VRAM word address snapped
/// DOWN to the 0x1000-word (8KB) hardware step and wrapped into VRAM (mod
/// 0x8000 words). Stored as the snapped EFFECTIVE word address; the bit-field
/// is `char_base >> 12` (the raw 4-bit field's aliasing top bit is not
/// preserved, as with `bg_map_base`).
#[inline]
pub fn bg_char_base(v: u32) -> u16 {
    (((v >> 12) & 0x07) << 12) as u16
}

/// M7SEL screen-over behavior (bits 6-7): 0/1 = wrap, 2 = transparent outside
/// the 1024x1024 map, 3 = fill with tile 0. Masks to 2 bits.
#[inline]
pub fn m7_repeat(v: u8) -> u8 {
    v & 0x03
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

    #[test]
    fn bg_tile_size_snaps_to_8_or_16() {
        assert_eq!(bg_tile_size(8), 8);
        assert_eq!(bg_tile_size(16), 16);
        assert_eq!(bg_tile_size(0), 8); // anything below 16 -> 8x8
        assert_eq!(bg_tile_size(255), 16); // anything >= 16 -> 16x16
    }

    #[test]
    fn bg_screen_size_masks_to_2_bits() {
        assert_eq!(bg_screen_size(0), 0);
        assert_eq!(bg_screen_size(3), 3);
        assert_eq!(bg_screen_size(4), 0); // wraps (mask), NOT clamp
    }

    #[test]
    fn bg_map_base_snaps_to_1kword_steps() {
        assert_eq!(bg_map_base(0), 0);
        assert_eq!(bg_map_base(0x0400), 0x0400);
        assert_eq!(bg_map_base(0x07ff), 0x0400); // snaps down to the step
        assert_eq!(bg_map_base(0x7c00), 0x7c00); // top of the 6-bit field
        assert_eq!(bg_map_base(0x8000), 0); // wraps past VRAM (6-bit mask)
    }

    #[test]
    fn bg_char_base_snaps_to_4kword_steps() {
        assert_eq!(bg_char_base(0), 0);
        assert_eq!(bg_char_base(0x1000), 0x1000);
        assert_eq!(bg_char_base(0x1fff), 0x1000); // snaps down
        assert_eq!(bg_char_base(0x7000), 0x7000); // top of the 4-bit field
        assert_eq!(bg_char_base(0x8000), 0); // wraps (4-bit mask)
    }

    #[test]
    fn m7_repeat_masks_to_2_bits() {
        assert_eq!(m7_repeat(0), 0);
        assert_eq!(m7_repeat(3), 3);
        assert_eq!(m7_repeat(5), 1); // wraps (mask), NOT clamp
    }
}
