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

/// Composite every visible sprite for scanline `y` into a `width`-long row.
/// `Some(px)` where a sprite pixel shows; `None` where none does. Higher `prio`
/// wins; ties break to the lower OAM index. This is the function the E5
/// compositor overlays onto BG layers.
pub fn render_scanline(mem: &Memory, y: usize, width: usize) -> Vec<Option<SpritePixel>> {
    let mut row = vec![None; width];
    let Some(sheet) = mem.obj_sheet.as_ref().and_then(|id| mem.sources.get(id)) else {
        return row;
    };
    if sheet.width < TILE || sheet.height < TILE {
        return row;
    }
    let tiles_per_row = (sheet.width / TILE).max(1);

    for i in sprites_on_line(mem, y) {
        let o = &mem.oam[i];
        let dim = sprite_dim(o.size);
        let origin_x = (o.tile as u32 % tiles_per_row) * TILE;
        let origin_y = (o.tile as u32 / tiles_per_row) * TILE;

        let local_y = (y as i64 - o.y.floor() as i64) as u32; // 0..dim by binning
        let sample_y = if o.flip_y { dim - 1 - local_y } else { local_y };
        let sy = origin_y + sample_y;
        if sy >= sheet.height {
            continue;
        }
        let pal_base = OBJ_CGRAM_BASE + (o.pal.min(7) as usize) * PALETTE_LEN;
        let left = o.x.floor() as i64;

        for local_x in 0..dim {
            let px = left + local_x as i64;
            if px < 0 || px as usize >= width {
                continue;
            }
            let sample_x = if o.flip_x { dim - 1 - local_x } else { local_x };
            let sx = origin_x + sample_x;
            if sx >= sheet.width {
                continue;
            }
            let red = sheet.rgba[((sy * sheet.width + sx) * 4) as usize];
            let index = (red & 0x0f) as usize;
            if index == 0 {
                continue; // transparent
            }
            let dst = &mut row[px as usize];
            if dst.map_or(true, |cur| o.prio > cur.prio) {
                let color = unpack_rgb15(mem.cgram[pal_base + index]);
                *dst = Some(SpritePixel { rgba: color, prio: o.prio });
            }
        }
    }
    row
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

    #[test]
    fn off_sprite_renders_nothing() {
        let mut mem = mem_with_sheet(2, 2, 1);
        mem.oam[0] = Obj { on: false, x: 0.0, y: 0.0, size: 0, ..Obj::default() };
        assert!(render_scanline(&mem, 0, 32).iter().all(|p| p.is_none()));
    }

    #[test]
    fn opaque_pixels_resolve_through_pal_and_cgram() {
        // Sheet index 5 everywhere; pal 0 -> colour at cgram[128 + 0*16 + 5].
        let mut mem = mem_with_sheet(2, 2, 5);
        mem.oam[0] = Obj { on: true, x: 3.0, y: 0.0, size: 0, pal: 0, ..Obj::default() };
        let row = render_scanline(&mem, 0, 32);
        let expected = unpack_rgb15(mem.cgram[128 + 5]);
        // covered: x in 3..11; uncovered elsewhere.
        assert_eq!(row[2], None);
        assert_eq!(row[3].unwrap().rgba, expected);
        assert_eq!(row[10].unwrap().rgba, expected);
        assert_eq!(row[11], None);
    }

    #[test]
    fn pal_selects_a_different_cgram_window() {
        let mut mem = mem_with_sheet(2, 2, 5);
        // Make pal 1's index-5 entry distinct from pal 0's.
        mem.cgram[128 + 16 + 5] = rgb15(255, 0, 0);
        mem.oam[0] = Obj { on: true, x: 0.0, y: 0.0, size: 0, pal: 1, ..Obj::default() };
        assert_eq!(render_scanline(&mem, 0, 32)[0].unwrap().rgba, unpack_rgb15(rgb15(255, 0, 0)));
    }

    #[test]
    fn index_zero_is_transparent() {
        let mut mem = mem_with_sheet(2, 2, 0); // every sheet pixel index 0
        mem.oam[0] = Obj { on: true, x: 0.0, y: 0.0, size: 0, ..Obj::default() };
        assert!(render_scanline(&mem, 0, 32).iter().all(|p| p.is_none()));
    }

    #[test]
    fn flips_mirror_within_the_sprite_block() {
        // 8x8 tile: left half index 1, right half index 2 (distinct colours).
        let w = 16u32;
        let mut rgba = vec![0u8; (w * 16 * 4) as usize];
        for y in 0..16u32 {
            for x in 0..16u32 {
                let i = ((y * w + x) * 4) as usize;
                rgba[i] = if x % 8 < 4 { 1 } else { 2 };
                rgba[i + 3] = 255;
            }
        }
        let mut mem = Memory::new();
        mem.sources.insert("sheet".into(), Source { width: w, height: 16, rgba });
        mem.obj_sheet = Some("sheet".into());
        mem.cgram[128 + 1] = rgb15(10, 10, 10);
        mem.cgram[128 + 2] = rgb15(250, 250, 250);
        let c1 = unpack_rgb15(rgb15(10, 10, 10));
        let c2 = unpack_rgb15(rgb15(250, 250, 250));

        mem.oam[0] = Obj { on: true, x: 0.0, y: 0.0, size: 0, ..Obj::default() };
        let row = render_scanline(&mem, 0, 32);
        assert_eq!(row[0].unwrap().rgba, c1); // left = index 1
        assert_eq!(row[7].unwrap().rgba, c2); // right = index 2

        mem.oam[0].flip_x = true;
        let row = render_scanline(&mem, 0, 32);
        assert_eq!(row[0].unwrap().rgba, c2); // mirrored
        assert_eq!(row[7].unwrap().rgba, c1);
    }
}
