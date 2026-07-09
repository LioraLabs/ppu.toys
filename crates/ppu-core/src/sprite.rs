//! Sprite (OBJ) rasterizer: per-scanline OAM binning, then 4bpp pixel
//! sampling from the OBSEL char base in VRAM through OBJ CGRAM (palettes
//! 8-15). Brightness is applied once by the E5 compositor.

use crate::bg::char_pixel_index;
use crate::memory::{unpack_rgb15, Memory};

/// SNES OBJ-per-scanline limit. Sprites beyond this many covering a line are
/// dropped in OAM-index order (lowest index kept).
pub const MAX_SPRITES_PER_LINE: usize = 32;

/// One composited sprite pixel: resolved colour plus the sprite's priority so
/// the downstream BG/OBJ compositor (E5) can interleave it with BG layers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpritePixel {
    pub rgba: [u8; 4],
    pub prio: u8,
    /// OBJ sub-palette 0..7 (needed by color math's palette-4-7 gate).
    pub pal: u8,
}

/// Authentic OBSEL size-pair table: `size_sel` (0..7) -> [(small W,H), (large W,H)].
const OBJ_SIZE_PAIRS: [[(u32, u32); 2]; 8] = [
    [(8, 8), (16, 16)],
    [(8, 8), (32, 32)],
    [(8, 8), (64, 64)],
    [(16, 16), (32, 32)],
    [(16, 16), (64, 64)],
    [(32, 32), (64, 64)],
    [(16, 32), (32, 64)],
    [(16, 32), (32, 32)],
];

/// (width, height) in pixels for a sprite, from the frame `size_sel` (OBSEL bits
/// 5-7) and the per-OAM `large` bit (OAM high table). `size_sel` masked to 3 bits.
fn sprite_dims(size_sel: u8, large: bool) -> (u32, u32) {
    OBJ_SIZE_PAIRS[(size_sel & 7) as usize][large as usize]
}

/// OAM indices of the (at most [`MAX_SPRITES_PER_LINE`]) sprites that are `on`
/// and cover scanline `y`, in ascending OAM order. Deterministic per-line
/// binning: lowest indices win when the line is over-subscribed.
pub fn sprites_on_line(mem: &Memory, y: usize) -> Vec<usize> {
    let y = y as i64;
    let size_sel = mem.obsel.size_sel;
    let mut out = Vec::with_capacity(MAX_SPRITES_PER_LINE);
    for (i, o) in mem.oam.iter().enumerate() {
        if !o.on {
            continue;
        }
        let top = o.y as i64;
        let (_w, h) = sprite_dims(size_sel, o.large);
        if y >= top && y < top + h as i64 {
            out.push(i);
            if out.len() == MAX_SPRITES_PER_LINE {
                break;
            }
        }
    }
    out
}

/// One OBJ tile is 4bpp: 16 VRAM words. Name-table addressing (16-wide row wrap,
/// row step, second-table gap) lives in [`obj_tile_addr`] below.
const OBJ_WORDS_PER_TILE: u32 = 16;

/// VRAM word address of the OBJ tile at block offset (`col`, `row`) from the
/// sprite's base `tile`, in the 16-wide OBJ name table. Right (+col) wraps within
/// the tile's own 16-tile row (`(name & 0x1f0) | ((name + col) & 0xf)`); down
/// (+row) steps +16, wrapping the 9-bit name. Names >= 256 live in the second
/// name table at `char_base + (name_select + 1) * 0x1000` (word-addressed).
fn obj_tile_addr(char_base: u32, name_select: u32, tile: u16, col: u32, row: u32) -> u16 {
    let row_tile = (tile as u32 + row * 16) & 0x1ff;
    let name = (row_tile & 0x1f0) | ((row_tile + col) & 0x0f);
    let addr = if name < 256 {
        char_base + name * OBJ_WORDS_PER_TILE
    } else {
        char_base + (name_select + 1) * 0x1000 + (name - 256) * OBJ_WORDS_PER_TILE
    };
    (addr & 0x7fff) as u16
}

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
    let size_sel = mem.obsel.size_sel;
    let name_select = mem.obsel.name_select as u32;
    for i in sprites_on_line(mem, y) {
        let o = mem.oam[i];
        let (w, h) = sprite_dims(size_sel, o.large);
        let row = (y as i64 - o.y as i64) as u32; // 0..h (binning guarantees in-range)
        let pal_base = 128 + (o.pal as usize & 7) * 16;
        for sx in 0..w {
            let screen_x = o.x as i64 + sx as i64;
            if screen_x < 0 || screen_x >= width as i64 {
                continue;
            }
            let slot = &mut out[screen_x as usize];
            if slot.is_some() {
                continue; // a lower OAM index already painted this pixel
            }
            let px = if o.flip_x { w - 1 - sx } else { sx };
            let py = if o.flip_y { h - 1 - row } else { row };
            let addr = obj_tile_addr(char_base, name_select, o.tile, px / 8, py / 8);
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
            large: false,
            ..Obj::default()
        };
        assert_eq!(sprites_on_line(&mem, 10), vec![0]);
    }

    #[test]
    fn sprite_dims_size_pair_table() {
        // sel 0: small 8x8 / large 16x16
        assert_eq!(sprite_dims(0, false), (8, 8));
        assert_eq!(sprite_dims(0, true), (16, 16));
        // sel 2: small 8x8 / large 64x64
        assert_eq!(sprite_dims(2, true), (64, 64));
        // sel 6: rectangular — small 16x32 / large 32x64
        assert_eq!(sprite_dims(6, false), (16, 32));
        assert_eq!(sprite_dims(6, true), (32, 64));
        // sel 7: small 16x32 / large 32x32
        assert_eq!(sprite_dims(7, false), (16, 32));
        assert_eq!(sprite_dims(7, true), (32, 32));
        assert_eq!(sprite_dims(8, false), (8, 8)); // size_sel masked to 3 bits
    }

    #[test]
    fn binning_selects_only_on_sprites_covering_the_line() {
        let mut mem = mem();
        mem.oam[0] = Obj {
            on: true,
            x: 0,
            y: 10,
            large: false,
            ..Obj::default()
        }; // rows 10..18
        mem.oam[1] = Obj {
            on: false,
            x: 0,
            y: 10,
            large: false,
            ..Obj::default()
        }; // off
        mem.oam[2] = Obj {
            on: true,
            x: 0,
            y: 100,
            large: false,
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
                large: false,
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
            large: false,
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
            large: true,
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

    #[test]
    fn obj_tile_addr_wraps_column_within_16_row() {
        // Base tile at column nibble 0xF: right (+1 col) wraps to column 0 of the
        // SAME 16-tile row (name 0x0F -> 0x00), NOT a naive carry into 0x10.
        let cb = 0x2000;
        assert_eq!(obj_tile_addr(cb, 0, 0x0f, 0, 0), 0x2000 + 0x0f * 16);
        assert_eq!(obj_tile_addr(cb, 0, 0x0f, 1, 0), 0x2000 + 0x00 * 16); // wrapped
                                                                          // Down (+1 row) steps +16 within the name space.
        assert_eq!(obj_tile_addr(cb, 0, 0x0f, 0, 1), 0x2000 + 0x1f * 16);
    }

    #[test]
    fn obj_tile_addr_second_nametable_uses_name_select_gap() {
        // Name >= 256 lands in the second table at char_base + (name_select+1)*0x1000.
        let cb = 0x2000;
        // Tile 0xF0 down 16 rows -> name 0xF0 + 16*16 = 0x1F0 (>=256).
        // name_select 0: gap 0x1000 -> addr = 0x2000 + 0x1000 + (0x1F0-0x100)*16.
        let name = 0x1f0u32;
        assert_eq!(
            obj_tile_addr(cb, 0, 0xf0, 0, 16),
            ((0x2000 + 0x1000 + (name - 0x100) * 16) & 0x7fff) as u16
        );
        // name_select 2: gap (2+1)*0x1000 = 0x3000.
        assert_eq!(
            obj_tile_addr(cb, 2, 0xf0, 0, 16),
            ((0x2000 + 0x3000 + (name - 0x100) * 16) & 0x7fff) as u16
        );
    }

    #[test]
    fn rectangular_sprite_samples_full_wxh_block() {
        let mut mem = obj_mem();
        mem.obsel.size_sel = 6; // large -> 32x64 (4 tiles wide, 8 tall)
        mem.cgram[128 + 1] = rgb15(255, 0, 0);
        // Put a marker pixel at (0,0) of the bottom-right block tile of a 32x64 sprite:
        // col 3 (x 24..31), row 7 (y 56..63) -> tile (0 + 7*16) then +3 col = tile 115.
        let mut g = [[0u8; 8]; 8];
        g[0][0] = 1;
        put_obj_char(&mut mem, 115, g);
        mem.oam[0] = Obj {
            on: true,
            x: 0,
            y: 0,
            tile: 0,
            large: true,
            ..Obj::default()
        };
        // The marker sits at screen (24, 56): far right column, bottom row of the block.
        assert!(render_scanline(&mem, 56, crate::WIDTH)[24].is_some());
        // The sprite is 64 tall, so row 63 is still covered; row 64 is not.
        assert_eq!(sprites_on_line(&mem, 63), vec![0]);
        assert_eq!(sprites_on_line(&mem, 64), Vec::<usize>::new());
    }
}
