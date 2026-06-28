//! Sprite (OBJ) rasterizer: per-scanline binning over the 128-entry OAM,
//! indexed into the global `obj.sheet`, composited by priority.
//!
//! Clean (NOT byte-accurate) model. The OBJ sheet is treated as a 4bpp indexed
//! image: each sheet pixel's low red nibble is its 0..15 colour index (index 0 =
//! transparent). A sprite's final colour is `cgram[128 + pal*16 + index]`,
//! matching the SNES convention that OBJ palettes occupy the upper half of CGRAM
//! (8 palettes of 16). This keeps `pal`, `cgram`, `unpack_rgb15`, flips, size and
//! priority all load-bearing. (BG full-image sources are direct-RGBA; OBJ are
//! paletted because the DSL gives every sprite a `pal` selector.)

use crate::memory::{unpack_rgb15, Memory};

/// SNES OBJ-per-scanline limit. Sprites beyond this many covering a line are
/// dropped in OAM-index order (lowest index kept).
pub const MAX_SPRITES_PER_LINE: usize = 32;

/// First CGRAM entry of the OBJ palette region (upper half of CGRAM).
const OBJ_CGRAM_BASE: usize = 128;
/// Colours per OBJ palette (4bpp).
const PALETTE_LEN: usize = 16;
/// Edge length of one OBJ tile cell, in pixels.
const TILE: u32 = 8;

/// One composited sprite pixel: resolved colour plus the sprite's priority so
/// the downstream BG/OBJ compositor (E5) can interleave it with BG layers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpritePixel {
    pub rgba: [u8; 4],
    pub prio: u8,
}

/// Pixel edge length of a sprite given its size selector: 8, 16, 32, 64.
fn sprite_dim(size: u8) -> u32 {
    TILE << (size.min(3) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sprite_dim_maps_size_selector() {
        assert_eq!(sprite_dim(0), 8);
        assert_eq!(sprite_dim(1), 16);
        assert_eq!(sprite_dim(2), 32);
        assert_eq!(sprite_dim(3), 64);
        assert_eq!(sprite_dim(9), 64); // clamped
    }
}
