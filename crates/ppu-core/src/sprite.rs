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

/// OAM indices of the (at most [`MAX_SPRITES_PER_LINE`]) sprites that are `on`
/// and cover scanline `y`, in ascending OAM order. Deterministic per-line
/// binning: lowest indices win when the line is over-subscribed.
pub fn sprites_on_line(mem: &Memory, y: usize) -> Vec<usize> {
    let y = y as i64;
    let mut out = Vec::with_capacity(MAX_SPRITES_PER_LINE);
    for (i, o) in mem.oam.iter().enumerate() {
        if !o.on {
            continue;
        }
        let top = o.y.floor() as i64;
        let dim = sprite_dim(o.size) as i64;
        if y >= top && y < top + dim {
            out.push(i);
            if out.len() == MAX_SPRITES_PER_LINE {
                break;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{rgb15, Source};
    use crate::registers::Obj;

    #[test]
    fn sprite_dim_maps_size_selector() {
        assert_eq!(sprite_dim(0), 8);
        assert_eq!(sprite_dim(1), 16);
        assert_eq!(sprite_dim(2), 32);
        assert_eq!(sprite_dim(3), 64);
        assert_eq!(sprite_dim(9), 64); // clamped
    }

    /// Build a Memory with a sheet of `tpr*8` × `rows*8` px, each pixel's red
    /// nibble = `index`, and an OBJ palette so colour index i -> a distinct hue.
    fn mem_with_sheet(tpr: u32, rows: u32, index: u8) -> Memory {
        let (w, h) = (tpr * 8, rows * 8);
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for px in rgba.chunks_mut(4) {
            px[0] = index; // colour index in low nibble
            px[3] = 255;
        }
        let mut mem = Memory::new();
        mem.sources
            .insert("sheet".into(), Source { width: w, height: h, rgba });
        mem.obj_sheet = Some("sheet".into());
        // OBJ palette 0 (cgram 128..144): index i -> grey ramp so it's non-zero.
        for i in 1..16u8 {
            mem.cgram[128 + i as usize] = rgb15(i * 16, i * 16, i * 16);
        }
        mem
    }

    #[test]
    fn binning_selects_only_on_sprites_covering_the_line() {
        let mut mem = mem_with_sheet(2, 2, 1);
        mem.oam[0] = Obj { on: true, x: 0.0, y: 10.0, size: 0, ..Obj::default() }; // rows 10..18
        mem.oam[1] = Obj { on: false, x: 0.0, y: 10.0, size: 0, ..Obj::default() }; // off
        mem.oam[2] = Obj { on: true, x: 0.0, y: 100.0, size: 0, ..Obj::default() }; // elsewhere
        assert_eq!(sprites_on_line(&mem, 12), vec![0]);
        assert_eq!(sprites_on_line(&mem, 9), Vec::<usize>::new());
        assert_eq!(sprites_on_line(&mem, 18), Vec::<usize>::new()); // exclusive bottom
    }

    #[test]
    fn binning_caps_at_max_per_line_keeping_lowest_indices() {
        let mut mem = mem_with_sheet(2, 2, 1);
        for i in 0..40usize {
            mem.oam[i] = Obj { on: true, x: 0.0, y: 0.0, size: 0, ..Obj::default() };
        }
        let on = sprites_on_line(&mem, 0);
        assert_eq!(on.len(), MAX_SPRITES_PER_LINE);
        assert_eq!(on.first(), Some(&0));
        assert_eq!(on.last(), Some(&(MAX_SPRITES_PER_LINE - 1)));
    }
}
