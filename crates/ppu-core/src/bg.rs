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

/// Render one BG layer for scanline `y` into `width` pixel candidates.
/// `None` = transparent at that x (lower layer / backdrop shows through).
/// Honors `.visible`, `.source` presence/lookup, per-layer scroll (wrapped over
/// the whole source image), the paletted-source contract, and brightness.
pub fn render_bg_layer_scanline(
    layer: &Bg,
    mem: &Memory,
    brightness: u8,
    y: usize,
    width: usize,
) -> Vec<Option<[u8; 4]>> {
    let mut out = vec![None; width];
    if !layer.visible {
        return out;
    }
    let Some(src_id) = layer.source.as_deref() else {
        return out;
    };
    let Some(src) = mem.sources.get(src_id) else {
        return out;
    };
    if src.width == 0 || src.height == 0 {
        return out;
    }
    let sw = src.width as i64;
    let sh = src.height as i64;
    let sy = ((y as f32 + layer.scroll_y).floor() as i64).rem_euclid(sh);
    for (x, slot) in out.iter_mut().enumerate() {
        let sx = ((x as f32 + layer.scroll_x).floor() as i64).rem_euclid(sw);
        let base = ((sy * sw + sx) * 4) as usize;
        if src.rgba[base + 3] == 0 {
            continue; // alpha 0 -> transparent
        }
        let pal = src.rgba[base]; // red channel = CGRAM index
        if pal == 0 {
            continue; // SNES color-0 -> transparent
        }
        let color = unpack_rgb15(mem.cgram[pal as usize]);
        *slot = Some(attenuate(color, brightness));
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

    use crate::memory::{rgb15, Source};

    // A 2x2 paletted source: red-channel = CGRAM index.
    // row0: (0,0)->idx1 opaque, (1,0)->idx2 opaque
    // row1: (0,1)->idx0 (transparent), (1,1)->idx3 alpha0 (transparent)
    fn fixture_mem() -> Memory {
        let mut m = Memory::new();
        m.cgram[1] = rgb15(255, 0, 0);
        m.cgram[2] = rgb15(0, 255, 0);
        m.cgram[3] = rgb15(0, 0, 255);
        let rgba = vec![
            1, 0, 0, 255, // (0,0) index 1 opaque
            2, 0, 0, 255, // (1,0) index 2 opaque
            0, 0, 0, 255, // (0,1) index 0 -> transparent
            3, 0, 0, 0, // (1,1) alpha 0 -> transparent
        ];
        m.sources.insert("bg".into(), Source { width: 2, height: 2, rgba });
        m
    }

    fn layer(source: &str) -> Bg {
        Bg { scroll_x: 0.0, scroll_y: 0.0, source: Some(source.into()), visible: true }
    }

    #[test]
    fn invisible_or_missing_source_is_all_transparent() {
        let m = fixture_mem();
        let mut off = layer("bg");
        off.visible = false;
        assert!(render_bg_layer_scanline(&off, &m, 15, 0, 4).iter().all(|p| p.is_none()));
        let no_src = Bg { source: None, ..layer("bg") };
        assert!(render_bg_layer_scanline(&no_src, &m, 15, 0, 4).iter().all(|p| p.is_none()));
        let bad = layer("missing");
        assert!(render_bg_layer_scanline(&bad, &m, 15, 0, 4).iter().all(|p| p.is_none()));
    }

    #[test]
    fn samples_palette_and_wraps_horizontally() {
        let m = fixture_mem();
        // scanline y=0 -> source row 0: [idx1=red, idx2=green], wraps over width 4.
        let line = render_bg_layer_scanline(&layer("bg"), &m, 15, 0, 4);
        assert_eq!(line[0], Some([255, 0, 0, 255])); // idx1 red
        assert_eq!(line[1], Some([0, 255, 0, 255])); // idx2 green
        assert_eq!(line[2], Some([255, 0, 0, 255])); // wrap -> idx1 red
        assert_eq!(line[3], Some([0, 255, 0, 255])); // wrap -> idx2 green
    }

    #[test]
    fn index0_and_alpha0_are_transparent() {
        let m = fixture_mem();
        // scanline y=1 -> source row 1: [idx0 transparent, idx3 alpha0 transparent]
        let line = render_bg_layer_scanline(&layer("bg"), &m, 15, 1, 2);
        assert_eq!(line[0], None);
        assert_eq!(line[1], None);
    }

    #[test]
    fn scroll_offsets_and_brightness_attenuates() {
        let m = fixture_mem();
        let mut l = layer("bg");
        l.scroll_x = 1.0; // x=0 samples source col 1 (idx2 green)
        let line = render_bg_layer_scanline(&l, &m, 0, 0, 1);
        assert_eq!(line[0], Some([0, 0, 0, 255])); // green attenuated to black at brightness 0
        let line2 = render_bg_layer_scanline(&l, &m, 15, 0, 1);
        assert_eq!(line2[0], Some([0, 255, 0, 255])); // full brightness green
    }

    #[test]
    fn negative_scroll_wraps() {
        let m = fixture_mem();
        let mut l = layer("bg");
        l.scroll_x = -1.0; // x=0 -> col -1 -> wraps to col 1 (idx2 green) on row 0
        let line = render_bg_layer_scanline(&l, &m, 15, 0, 1);
        assert_eq!(line[0], Some([0, 255, 0, 255]));
    }
}
