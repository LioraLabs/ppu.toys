//! Sprite (OBJ) rasterizer: per-scanline binning over the 128-entry OAM,
//! indexed into the global `obj.sheet`, composited by priority.
//!
//! `render_scanline` (pixel sampling) is a STUB pending m4/compositing / m4/demos:
//! the v1 direct-RGBA OBJ sheet was deleted by m4/memory. OAM state and
//! per-line binning (`sprites_on_line`) remain fully functional and tested.
//! Brightness is applied once by the E5 compositor.

use crate::memory::{unpack_rgb15, Memory};

/// SNES OBJ-per-scanline limit. Sprites beyond this many covering a line are
/// dropped in OAM-index order (lowest index kept).
pub const MAX_SPRITES_PER_LINE: usize = 32;

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
        let top = o.y as i64;
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

/// Composite every visible sprite for scanline `y` into a `width`-long row.
///
/// TODO(m4/compositing, m4/demos): STUB during the M4 substrate rewrite — the v1
/// direct-RGBA OBJ sheet (`Memory::sources`) was deleted by m4/memory and M4
/// defines no sprite-through-VRAM ticket; sprite pixel sampling returns with
/// the compositing (m4/compositing) / demo re-enable (m4/demos) work. OAM state and
/// per-line binning (`sprites_on_line`) remain fully functional.
pub fn render_scanline(mem: &Memory, y: usize, width: usize) -> Vec<Option<SpritePixel>> {
    let _ = sprites_on_line(mem, y); // binning stays exercised
    vec![None; width]
}

/// Full-frame sprite raster over the CGRAM backdrop (`cgram[0]`), for golden
/// tests. The real compositor (E5) overlays [`render_scanline`] onto BG layers
/// instead of this flat backdrop.
pub fn render_sprites(mem: &Memory, width: usize, height: usize) -> Vec<u8> {
    let backdrop = unpack_rgb15(mem.cgram[0]);
    let mut fb = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        for cell in render_scanline(mem, y, width) {
            fb.extend_from_slice(&cell.map_or(backdrop, |p| p.rgba));
        }
    }
    fb
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::rgb15;
    use crate::registers::Obj;

    // TODO(m4/compositing, m4/demos): pixel-sampling tests (colors, flips, priority ties)
    // were deleted with the v1 direct-RGBA sheet; they return when sprites sample
    // real memory again.

    fn mem() -> Memory {
        Memory::new()
    }

    #[test]
    fn obj_coords_are_integer_registers() {
        let mut mem = mem();
        mem.oam[0] = Obj { on: true, x: 5, y: 10, size: 0, ..Obj::default() };
        assert_eq!(sprites_on_line(&mem, 10), vec![0]);
    }

    #[test]
    fn sprite_dim_maps_size_selector() {
        assert_eq!(sprite_dim(0), 8);
        assert_eq!(sprite_dim(1), 16);
        assert_eq!(sprite_dim(2), 32);
        assert_eq!(sprite_dim(3), 64);
        assert_eq!(sprite_dim(9), 64); // clamped
    }

    #[test]
    fn binning_selects_only_on_sprites_covering_the_line() {
        let mut mem = mem();
        mem.oam[0] = Obj { on: true, x: 0, y: 10, size: 0, ..Obj::default() }; // rows 10..18
        mem.oam[1] = Obj { on: false, x: 0, y: 10, size: 0, ..Obj::default() }; // off
        mem.oam[2] = Obj { on: true, x: 0, y: 100, size: 0, ..Obj::default() }; // elsewhere
        assert_eq!(sprites_on_line(&mem, 12), vec![0]);
        assert_eq!(sprites_on_line(&mem, 9), Vec::<usize>::new());
        assert_eq!(sprites_on_line(&mem, 18), Vec::<usize>::new()); // exclusive bottom
    }

    #[test]
    fn binning_caps_at_max_per_line_keeping_lowest_indices() {
        let mut mem = mem();
        for i in 0..40usize {
            mem.oam[i] = Obj { on: true, x: 0, y: 0, size: 0, ..Obj::default() };
        }
        let on = sprites_on_line(&mem, 0);
        assert_eq!(on.len(), MAX_SPRITES_PER_LINE);
        assert_eq!(on.first(), Some(&0));
        assert_eq!(on.last(), Some(&(MAX_SPRITES_PER_LINE - 1)));
    }

    #[test]
    fn stub_scanline_renders_no_sprite_pixels() {
        let mut mem = mem();
        mem.oam[0] = Obj { on: true, x: 0, y: 0, size: 0, ..Obj::default() };
        assert!(render_scanline(&mem, 0, 32).iter().all(|p| p.is_none()));
    }

    #[test]
    fn render_sprites_is_full_size_opaque_and_uses_backdrop() {
        let mut mem = mem();
        mem.cgram[0] = rgb15(0, 0, 40); // backdrop
        let fb = render_sprites(&mem, 32, 32);
        assert_eq!(fb.len(), 32 * 32 * 4);
        assert!(fb.chunks(4).all(|px| px[3] == 255));
        assert_eq!(&fb[0..4], &unpack_rgb15(rgb15(0, 0, 40)));
    }
}
