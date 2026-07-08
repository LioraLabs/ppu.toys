//! Sprite (OBJ) rasterizer: per-scanline OAM binning, then 4bpp pixel
//! sampling from the OBSEL char base in VRAM through OBJ CGRAM (palettes
//! 8-15). Brightness is applied once by the E5 compositor.

use crate::bg::char_pixel_index;
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
    /// OBJ sub-palette 0..7 (needed by color math's palette-4-7 gate).
    pub pal: u8,
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

/// One OBJ tile is 4bpp: 16 VRAM words. Sprites index the OBJ name table 16
/// tiles wide (right = +1, down = +16), masked to 9 bits.
const OBJ_WORDS_PER_TILE: u32 = 16;

/// Composite every visible sprite for scanline `y` into a `width`-long row of
/// `Option<SpritePixel>` (`None` = transparent). Real OBJ pixel sampling: per
/// covering sprite (lowest OAM index first), fetch its 4bpp char from the OBSEL
/// char base in VRAM, apply flip/size, decode the palette index, and map index
/// 0 = transparent / else `cgram[128 + pal*16 + index]` (OBJ palettes 8-15).
/// `SpritePixel.prio` carries the sprite's priority for the BG/OBJ compositor.
/// Un-attenuated; brightness is applied by the compositor.
pub fn render_scanline(mem: &Memory, y: usize, width: usize) -> Vec<Option<SpritePixel>> {
    let mut out = vec![None; width];
    let char_base = mem.obsel.char_base as u32;
    for i in sprites_on_line(mem, y) {
        let o = mem.oam[i];
        let dim = sprite_dim(o.size);
        let row = (y as i64 - o.y as i64) as u32; // 0..dim (binning guarantees in-range)
        let pal_base = 128 + (o.pal as usize & 7) * 16;
        for sx in 0..dim {
            let screen_x = o.x as i64 + sx as i64;
            if screen_x < 0 || screen_x >= width as i64 {
                continue;
            }
            let slot = &mut out[screen_x as usize];
            if slot.is_some() {
                continue; // a lower OAM index already painted this pixel
            }
            let px = if o.flip_x { dim - 1 - sx } else { sx };
            let py = if o.flip_y { dim - 1 - row } else { row };
            // Name-table walk: +1 per column, +16 per row (masked to 9 bits).
            // Hardware wraps the column within the tile's own 16-wide row
            // (`(tile & 0x1f0) | ((tile + col) & 0xf)`); the simpler carry here
            // only diverges when a wide sprite's base column nibble + width
            // crosses 16 — acceptable for this educational core.
            let tile_index = (o.tile as u32 + px / 8 + (py / 8) * 16) & 0x1ff;
            let addr = ((char_base + tile_index * OBJ_WORDS_PER_TILE) & 0x7fff) as u16;
            let index = char_pixel_index(mem, addr, 4, px % 8, py % 8);
            if index == 0 {
                continue; // color 0 = transparent
            }
            *slot = Some(SpritePixel {
                rgba: unpack_rgb15(mem.cgram[pal_base + index as usize]),
                prio: o.prio,
                pal: o.pal & 7,
            });
        }
    }
    out
}

/// Full-frame sprite raster over the CGRAM backdrop (`cgram[0]`), for sprite
/// unit tests: [`render_scanline`] sampled over a flat backdrop. The real E5
/// compositor overlays [`render_scanline`] onto BG layers instead of this
/// flat backdrop.
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

    fn mem() -> Memory {
        Memory::new()
    }

    #[test]
    fn obj_coords_are_integer_registers() {
        let mut mem = mem();
        mem.oam[0] = Obj {
            on: true,
            x: 5,
            y: 10,
            size: 0,
            ..Obj::default()
        };
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
        mem.oam[0] = Obj {
            on: true,
            x: 0,
            y: 10,
            size: 0,
            ..Obj::default()
        }; // rows 10..18
        mem.oam[1] = Obj {
            on: false,
            x: 0,
            y: 10,
            size: 0,
            ..Obj::default()
        }; // off
        mem.oam[2] = Obj {
            on: true,
            x: 0,
            y: 100,
            size: 0,
            ..Obj::default()
        }; // elsewhere
        assert_eq!(sprites_on_line(&mem, 12), vec![0]);
        assert_eq!(sprites_on_line(&mem, 9), Vec::<usize>::new());
        assert_eq!(sprites_on_line(&mem, 18), Vec::<usize>::new()); // exclusive bottom
    }

    #[test]
    fn binning_caps_at_max_per_line_keeping_lowest_indices() {
        let mut mem = mem();
        for i in 0..40usize {
            mem.oam[i] = Obj {
                on: true,
                x: 0,
                y: 0,
                size: 0,
                ..Obj::default()
            };
        }
        let on = sprites_on_line(&mem, 0);
        assert_eq!(on.len(), MAX_SPRITES_PER_LINE);
        assert_eq!(on.first(), Some(&0));
        assert_eq!(on.last(), Some(&(MAX_SPRITES_PER_LINE - 1)));
    }

    /// Write a 4bpp OBJ char (16 words) at OBJ char base 0x2000, tile `n`,
    /// from an 8x8 index grid (index 0..15).
    fn put_obj_char(mem: &mut Memory, n: usize, grid: [[u8; 8]; 8]) {
        let base = 0x2000 + n * 16;
        for y in 0..8 {
            let (mut p01, mut p23) = (0u16, 0u16);
            for x in 0..8 {
                let v = grid[y][x] as u16;
                let bit = 7 - x;
                p01 |= (v & 1) << bit | ((v >> 1) & 1) << (bit + 8);
                p23 |= ((v >> 2) & 1) << bit | ((v >> 3) & 1) << (bit + 8);
            }
            mem.vram[base + y] = p01;
            mem.vram[base + 8 + y] = p23;
        }
    }

    fn obj_mem() -> Memory {
        let mut mem = Memory::new();
        mem.obsel.char_base = 0x2000;
        mem
    }

    #[test]
    fn samples_4bpp_obj_tile_through_obj_cgram() {
        let mut mem = obj_mem();
        mem.cgram[128 + 1] = rgb15(255, 0, 0);
        let mut g = [[0u8; 8]; 8];
        g[0][0] = 1;
        put_obj_char(&mut mem, 1, g);
        mem.oam[0] = Obj {
            on: true,
            x: 10,
            y: 5,
            tile: 1,
            size: 0,
            ..Obj::default()
        };
        let line = render_scanline(&mem, 5, crate::WIDTH);
        let px = line[10].expect("sprite pixel at (10,5)");
        assert_eq!(px.rgba, unpack_rgb15(rgb15(255, 0, 0)));
        assert_eq!(px.prio, 0);
        assert!(line[11].is_none());
        assert!(render_scanline(&mem, 6, crate::WIDTH)[10].is_none());
    }

    #[test]
    fn palette_select_indexes_obj_cgram_bank() {
        let mut mem = obj_mem();
        mem.cgram[128 + 3 * 16 + 1] = rgb15(0, 255, 0);
        let mut g = [[0u8; 8]; 8];
        g[0][0] = 1;
        put_obj_char(&mut mem, 1, g);
        mem.oam[0] = Obj {
            on: true,
            x: 0,
            y: 0,
            tile: 1,
            pal: 3,
            ..Obj::default()
        };
        assert_eq!(
            render_scanline(&mem, 0, crate::WIDTH)[0].unwrap().rgba,
            unpack_rgb15(rgb15(0, 255, 0))
        );
    }

    #[test]
    fn flip_x_and_flip_y_mirror_the_sprite() {
        let mut mem = obj_mem();
        mem.cgram[128 + 1] = rgb15(255, 255, 255);
        let mut g = [[0u8; 8]; 8];
        g[0][0] = 1;
        put_obj_char(&mut mem, 1, g);
        mem.oam[0] = Obj {
            on: true,
            x: 0,
            y: 0,
            tile: 1,
            flip_x: true,
            ..Obj::default()
        };
        let line = render_scanline(&mem, 0, crate::WIDTH);
        assert!(line[0].is_none());
        assert!(line[7].is_some());
        mem.oam[0].flip_x = false;
        mem.oam[0].flip_y = true;
        assert!(render_scanline(&mem, 0, crate::WIDTH)[0].is_none());
        assert!(render_scanline(&mem, 7, crate::WIDTH)[0].is_some());
    }

    #[test]
    fn size1_16x16_addresses_four_quadrant_tiles() {
        let mut mem = obj_mem();
        for i in 1..=4u16 {
            mem.cgram[128 + i as usize] = rgb15(i as u8 * 40, 0, 0);
        }
        let corner = |v: u8| {
            let mut g = [[0u8; 8]; 8];
            g[0][0] = v;
            g
        };
        put_obj_char(&mut mem, 1, corner(1));
        put_obj_char(&mut mem, 2, corner(2));
        put_obj_char(&mut mem, 17, corner(3));
        put_obj_char(&mut mem, 18, corner(4));
        mem.oam[0] = Obj {
            on: true,
            x: 0,
            y: 0,
            tile: 1,
            size: 1,
            ..Obj::default()
        };
        let at = |mm: &Memory, y: usize, x: usize| {
            render_scanline(mm, y, crate::WIDTH)[x].map(|p| p.rgba)
        };
        assert_eq!(at(&mem, 0, 0), Some(unpack_rgb15(rgb15(40, 0, 0))));
        assert_eq!(at(&mem, 0, 8), Some(unpack_rgb15(rgb15(80, 0, 0))));
        assert_eq!(at(&mem, 8, 0), Some(unpack_rgb15(rgb15(120, 0, 0))));
        assert_eq!(at(&mem, 8, 8), Some(unpack_rgb15(rgb15(160, 0, 0))));
    }

    #[test]
    fn priority_field_is_carried() {
        let mut mem = obj_mem();
        mem.cgram[128 + 1] = rgb15(1, 2, 3);
        let mut g = [[0u8; 8]; 8];
        g[0][0] = 1;
        put_obj_char(&mut mem, 1, g);
        mem.oam[0] = Obj {
            on: true,
            x: 0,
            y: 0,
            tile: 1,
            prio: 2,
            ..Obj::default()
        };
        assert_eq!(render_scanline(&mem, 0, crate::WIDTH)[0].unwrap().prio, 2);
    }

    #[test]
    fn lower_oam_index_wins_overlap() {
        let mut mem = obj_mem();
        mem.cgram[128 + 1] = rgb15(255, 0, 0);
        mem.cgram[128 + 2] = rgb15(0, 0, 255);
        let mut g1 = [[0u8; 8]; 8];
        g1[0][0] = 1;
        let mut g2 = [[0u8; 8]; 8];
        g2[0][0] = 2;
        put_obj_char(&mut mem, 1, g1);
        put_obj_char(&mut mem, 2, g2);
        mem.oam[5] = Obj {
            on: true,
            x: 0,
            y: 0,
            tile: 2,
            ..Obj::default()
        };
        mem.oam[0] = Obj {
            on: true,
            x: 0,
            y: 0,
            tile: 1,
            ..Obj::default()
        };
        assert_eq!(
            render_scanline(&mem, 0, crate::WIDTH)[0].unwrap().rgba,
            unpack_rgb15(rgb15(255, 0, 0))
        );
    }

    #[test]
    fn negative_x_clips_off_left() {
        let mut mem = obj_mem();
        mem.cgram[128 + 1] = rgb15(255, 255, 255);
        let mut g = [[0u8; 8]; 8];
        g[7][7] = 1;
        put_obj_char(&mut mem, 1, g);
        mem.oam[0] = Obj {
            on: true,
            x: -7,
            y: 0,
            tile: 1,
            ..Obj::default()
        };
        assert!(render_scanline(&mem, 7, crate::WIDTH)[0].is_some());
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
