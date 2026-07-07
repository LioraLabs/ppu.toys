//! Mode-1 tile background rasterizer.
//!
//! Renders BG layers from their whole-image sources (`Bg::source`), auto-tiled
//! and wrapped. Direct-RGBA contract (v1): a source pixel's actual RGBA *is* the
//! graphic; `rgba[3] == 0` (alpha 0) is transparent so the lower layer / backdrop
//! shows through. CGRAM is NOT consulted for BG pixels in v1 (only `cgram[0]`
//! backdrop, handled by the compositor). Brightness is applied once by the E5
//! compositor; the per-layer primitive here returns un-attenuated color.

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
/// Honors `.visible`, `.source` presence/lookup, per-layer scroll (wrapped over
/// the whole source image). Returns DIRECT, un-attenuated source RGBA; the
/// compositor applies brightness once.
pub fn render_bg_layer_scanline(
    layer: &RegBg,
    mem: &Memory,
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
    let sy = (y as i64 + layer.scroll_y as i64).rem_euclid(sh);
    for (x, slot) in out.iter_mut().enumerate() {
        let sx = (x as i64 + layer.scroll_x as i64).rem_euclid(sw);
        let base = ((sy * sw + sx) * 4) as usize;
        if src.rgba[base + 3] == 0 {
            continue; // alpha 0 -> transparent
        }
        *slot = Some([
            src.rgba[base],
            src.rgba[base + 1],
            src.rgba[base + 2],
            255,
        ]);
    }
    out
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

    use crate::memory::{rgb15, Source};
    use crate::registers::{Bg, LineTableRow, RegRow};

    // A 2x2 direct-RGBA source:
    // row0: (0,0)=red opaque, (1,0)=green opaque
    // row1: (0,1)=alpha0 transparent, (1,1)=blue opaque
    fn fixture_mem() -> Memory {
        let mut m = Memory::new();
        let rgba = vec![
            255, 0, 0, 255, // (0,0) red opaque
            0, 255, 0, 255, // (1,0) green opaque
            0, 0, 0, 0, // (0,1) alpha 0 -> transparent
            0, 0, 255, 255, // (1,1) blue opaque
        ];
        m.sources.insert("bg".into(), Source { width: 2, height: 2, rgba });
        m
    }

    fn layer(source: &str) -> RegBg {
        RegBg::from(&Bg { source: Some(source.into()), ..Bg::default() })
    }

    #[test]
    fn invisible_or_missing_source_is_all_transparent() {
        let m = fixture_mem();
        let mut off = layer("bg");
        off.visible = false;
        assert!(render_bg_layer_scanline(&off, &m, 0, 4).iter().all(|p| p.is_none()));
        let no_src = RegBg { source: None, ..layer("bg") };
        assert!(render_bg_layer_scanline(&no_src, &m, 0, 4).iter().all(|p| p.is_none()));
        let bad = layer("missing");
        assert!(render_bg_layer_scanline(&bad, &m, 0, 4).iter().all(|p| p.is_none()));
    }

    #[test]
    fn samples_direct_color_and_wraps_horizontally() {
        let m = fixture_mem();
        // scanline y=0 -> source row 0: [red, green], wraps over width 4.
        let line = render_bg_layer_scanline(&layer("bg"), &m, 0, 4);
        assert_eq!(line[0], Some([255, 0, 0, 255])); // red
        assert_eq!(line[1], Some([0, 255, 0, 255])); // green
        assert_eq!(line[2], Some([255, 0, 0, 255])); // wrap -> red
        assert_eq!(line[3], Some([0, 255, 0, 255])); // wrap -> green
    }

    #[test]
    fn alpha0_is_transparent() {
        let m = fixture_mem();
        // scanline y=1 -> source row 1: [alpha0 transparent, blue opaque]
        let line = render_bg_layer_scanline(&layer("bg"), &m, 1, 2);
        assert_eq!(line[0], None);
        assert_eq!(line[1], Some([0, 0, 255, 255]));
    }

    #[test]
    fn scroll_offsets_the_sample() {
        let m = fixture_mem();
        let mut l = layer("bg");
        l.scroll_x = 1; // x=0 samples source col 1 (green) on row 0
        assert_eq!(render_bg_layer_scanline(&l, &m, 0, 1)[0], Some([0, 255, 0, 255]));
    }

    #[test]
    fn negative_scroll_wraps() {
        let m = fixture_mem();
        let mut l = layer("bg");
        l.scroll_x = -1; // x=0 -> col -1 -> wraps to col 1 (green) on row 0
        assert_eq!(render_bg_layer_scanline(&l, &m, 0, 1)[0], Some([0, 255, 0, 255]));
    }

    #[test]
    fn composite_shows_backdrop_where_transparent() {
        let mut m = fixture_mem();
        m.cgram[0] = rgb15(10, 20, 30); // backdrop
        let mut row = RegRow::from(&LineTableRow::default());
        row.brightness = 15;
        row.bg[0] = layer("bg");
        // row 1 col 0 of source is alpha0 -> backdrop shows through there.
        let line = render_bg_scanline(&row, &m, 1, 2);
        assert_eq!(line[0], unpack_rgb15(rgb15(10, 20, 30)));
        assert_eq!(line[1], [0, 0, 255, 255]); // blue opaque
    }

    #[test]
    fn composite_topmost_layer_wins() {
        let mut m = fixture_mem();
        m.cgram[0] = rgb15(0, 0, 0);
        let mut row = RegRow::from(&LineTableRow::default());
        row.brightness = 15;
        row.bg[0] = layer("bg"); // bg1 topmost
        row.bg[1] = layer("bg"); // bg2 below
        // both opaque red at col 0 row 0; bg1 wins (same color here, asserts opacity).
        assert_eq!(render_bg_scanline(&row, &m, 0, 1)[0], [255, 0, 0, 255]);
    }

    #[test]
    fn composite_applies_brightness_to_backdrop() {
        let mut m = fixture_mem();
        m.cgram[0] = rgb15(200, 200, 200);
        let mut row = RegRow::from(&LineTableRow::default());
        row.brightness = 0; // everything black
        assert_eq!(render_bg_scanline(&row, &m, 0, 2)[0], [0, 0, 0, 255]);
    }
}
