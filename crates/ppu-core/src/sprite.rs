//! Sprite (OBJ) rasterizer: per-scanline binning over the 128-entry OAM,
//! indexed into the global `obj.sheet`, composited by priority.
//!
//! Direct-RGBA contract (v1): the OBJ sheet's actual RGBA *is* the graphic.
//! `obj[i].tile` indexes the sheet in 8x8 cells; a sprite samples a
//! `dim x dim` block from there. `rgba[3] == 0` (alpha 0) is transparent.
//! CGRAM is NOT consulted for sprite pixels in v1. `Obj::pal` is a NO-OP in v1
//! (the field is kept for the DSL surface; per-palette recolor is v2).
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
            let si = ((sy * sheet.width + sx) * 4) as usize;
            if sheet.rgba[si + 3] == 0 {
                continue; // alpha 0 -> transparent
            }
            let dst = &mut row[px as usize];
            if dst.is_none_or(|cur| o.prio > cur.prio) {
                *dst = Some(SpritePixel {
                    rgba: [sheet.rgba[si], sheet.rgba[si + 1], sheet.rgba[si + 2], 255],
                    prio: o.prio,
                });
            }
        }
    }
    row
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

    /// Build a Memory with a `tpr*8` x `rows*8` sheet of a single solid RGBA
    /// `color` (alpha from `color[3]`), and an OBJ sheet selector.
    fn mem_with_sheet(tpr: u32, rows: u32, color: [u8; 4]) -> Memory {
        let (w, h) = (tpr * 8, rows * 8);
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for px in rgba.chunks_mut(4) {
            px.copy_from_slice(&color);
        }
        let mut mem = Memory::new();
        mem.sources
            .insert("sheet".into(), Source { width: w, height: h, rgba });
        mem.obj_sheet = Some("sheet".into());
        mem
    }

    #[test]
    fn binning_selects_only_on_sprites_covering_the_line() {
        let mut mem = mem_with_sheet(2, 2, [255, 255, 255, 255]);
        mem.oam[0] = Obj { on: true, x: 0.0, y: 10.0, size: 0, ..Obj::default() }; // rows 10..18
        mem.oam[1] = Obj { on: false, x: 0.0, y: 10.0, size: 0, ..Obj::default() }; // off
        mem.oam[2] = Obj { on: true, x: 0.0, y: 100.0, size: 0, ..Obj::default() }; // elsewhere
        assert_eq!(sprites_on_line(&mem, 12), vec![0]);
        assert_eq!(sprites_on_line(&mem, 9), Vec::<usize>::new());
        assert_eq!(sprites_on_line(&mem, 18), Vec::<usize>::new()); // exclusive bottom
    }

    #[test]
    fn binning_caps_at_max_per_line_keeping_lowest_indices() {
        let mut mem = mem_with_sheet(2, 2, [255, 255, 255, 255]);
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
        let mut mem = mem_with_sheet(2, 2, [255, 255, 255, 255]);
        mem.oam[0] = Obj { on: false, x: 0.0, y: 0.0, size: 0, ..Obj::default() };
        assert!(render_scanline(&mem, 0, 32).iter().all(|p| p.is_none()));
    }

    #[test]
    fn opaque_pixels_sample_direct_sheet_color() {
        let mut mem = mem_with_sheet(2, 2, [10, 200, 30, 255]);
        mem.oam[0] = Obj { on: true, x: 3.0, y: 0.0, size: 0, ..Obj::default() };
        let row = render_scanline(&mem, 0, 32);
        // covered: x in 3..11; uncovered elsewhere.
        assert_eq!(row[2], None);
        assert_eq!(row[3].unwrap().rgba, [10, 200, 30, 255]);
        assert_eq!(row[10].unwrap().rgba, [10, 200, 30, 255]);
        assert_eq!(row[11], None);
    }

    #[test]
    fn pal_is_a_noop_in_v1() {
        let mut a = mem_with_sheet(2, 2, [10, 200, 30, 255]);
        a.oam[0] = Obj { on: true, x: 0.0, y: 0.0, size: 0, pal: 0, ..Obj::default() };
        let mut b = mem_with_sheet(2, 2, [10, 200, 30, 255]);
        b.oam[0] = Obj { on: true, x: 0.0, y: 0.0, size: 0, pal: 7, ..Obj::default() };
        assert_eq!(
            render_scanline(&a, 0, 32)[0].unwrap().rgba,
            render_scanline(&b, 0, 32)[0].unwrap().rgba
        );
    }

    #[test]
    fn alpha_zero_is_transparent() {
        let mut mem = mem_with_sheet(2, 2, [200, 0, 0, 0]); // alpha 0 everywhere
        mem.oam[0] = Obj { on: true, x: 0.0, y: 0.0, size: 0, ..Obj::default() };
        assert!(render_scanline(&mem, 0, 32).iter().all(|p| p.is_none()));
    }

    #[test]
    fn flips_mirror_within_the_sprite_block() {
        // 8x8 tile: left half color A, right half color B (distinct, opaque).
        let w = 16u32;
        let a = [10, 10, 10, 255];
        let bcol = [250, 250, 250, 255];
        let mut rgba = vec![0u8; (w * 16 * 4) as usize];
        for y in 0..16u32 {
            for x in 0..16u32 {
                let i = ((y * w + x) * 4) as usize;
                rgba[i..i + 4].copy_from_slice(if x % 8 < 4 { &a } else { &bcol });
            }
        }
        let mut mem = Memory::new();
        mem.sources.insert("sheet".into(), Source { width: w, height: 16, rgba });
        mem.obj_sheet = Some("sheet".into());

        mem.oam[0] = Obj { on: true, x: 0.0, y: 0.0, size: 0, ..Obj::default() };
        let row = render_scanline(&mem, 0, 32);
        assert_eq!(row[0].unwrap().rgba, a); // left
        assert_eq!(row[7].unwrap().rgba, bcol); // right

        mem.oam[0].flip_x = true;
        let row = render_scanline(&mem, 0, 32);
        assert_eq!(row[0].unwrap().rgba, bcol); // mirrored
        assert_eq!(row[7].unwrap().rgba, a);
    }

    #[test]
    fn higher_prio_sprite_wins_overlap() {
        let mut mem = mem_with_sheet(2, 2, [10, 0, 0, 255]);
        mem.oam[0] = Obj { on: true, x: 0.0, y: 0.0, size: 0, prio: 0, ..Obj::default() };
        mem.oam[1] = Obj { on: true, x: 0.0, y: 0.0, size: 0, prio: 3, ..Obj::default() };
        assert_eq!(render_scanline(&mem, 0, 32)[0].unwrap().prio, 3);
    }

    #[test]
    fn equal_prio_keeps_lower_oam_index() {
        // Two distinct sheet halves so the winner's color is identifiable.
        let w = 16u32;
        let left = [10, 0, 0, 255];
        let right = [0, 250, 0, 255];
        let mut rgba = vec![0u8; (w * 16 * 4) as usize];
        for y in 0..16u32 {
            for x in 0..16u32 {
                let i = ((y * w + x) * 4) as usize;
                rgba[i..i + 4].copy_from_slice(if x % 8 < 4 { &left } else { &right });
            }
        }
        let mut mem = Memory::new();
        mem.sources.insert("sheet".into(), Source { width: w, height: 16, rgba });
        mem.obj_sheet = Some("sheet".into());
        // oam[0] reads tile 0 (left color); oam[1] reads tile 1 (right color),
        // both placed at x=0, equal prio -> lower index (oam[0]) wins.
        mem.oam[0] = Obj { on: true, x: 0.0, y: 0.0, tile: 0, size: 0, prio: 2, ..Obj::default() };
        mem.oam[1] = Obj { on: true, x: 0.0, y: 0.0, tile: 1, size: 0, prio: 2, ..Obj::default() };
        let px = render_scanline(&mem, 0, 32)[0].unwrap();
        assert_eq!(px.rgba, left); // oam[0] kept on tie
        assert_eq!(px.prio, 2);
    }

    #[test]
    fn flip_y_mirrors_vertically_within_the_sprite() {
        // 8x8 tile: top half color A, bottom half color B.
        let w = 16u32;
        let a = [10, 10, 10, 255];
        let bcol = [250, 250, 250, 255];
        let mut rgba = vec![0u8; (w * 16 * 4) as usize];
        for yy in 0..16u32 {
            for xx in 0..16u32 {
                let i = ((yy * w + xx) * 4) as usize;
                rgba[i..i + 4].copy_from_slice(if yy % 8 < 4 { &a } else { &bcol });
            }
        }
        let mut mem = Memory::new();
        mem.sources.insert("sheet".into(), Source { width: w, height: 16, rgba });
        mem.obj_sheet = Some("sheet".into());

        mem.oam[0] = Obj { on: true, x: 0.0, y: 0.0, size: 0, ..Obj::default() };
        assert_eq!(render_scanline(&mem, 0, 32)[0].unwrap().rgba, a);
        assert_eq!(render_scanline(&mem, 7, 32)[0].unwrap().rgba, bcol);

        mem.oam[0].flip_y = true;
        assert_eq!(render_scanline(&mem, 0, 32)[0].unwrap().rgba, bcol);
        assert_eq!(render_scanline(&mem, 7, 32)[0].unwrap().rgba, a);
    }

    #[test]
    fn render_sprites_is_full_size_opaque_and_uses_backdrop() {
        let mut mem = mem_with_sheet(2, 2, [255, 255, 255, 255]);
        mem.cgram[0] = rgb15(0, 0, 40); // backdrop
        mem.oam[0] = Obj { on: true, x: 4.0, y: 4.0, size: 0, ..Obj::default() };
        let fb = render_sprites(&mem, 32, 32);
        assert_eq!(fb.len(), 32 * 32 * 4);
        assert!(fb.chunks(4).all(|px| px[3] == 255));
        // top-left corner is backdrop (sprite starts at 4,4).
        assert_eq!(&fb[0..4], &unpack_rgb15(rgb15(0, 0, 40)));
    }
}
