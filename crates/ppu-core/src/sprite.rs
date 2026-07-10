//! Sprite (OBJ) rasterizer: per-scanline OAM binning, then 4bpp pixel
//! sampling from the OBSEL char base in VRAM through OBJ CGRAM (palettes
//! 8-15). Brightness is applied once by the E5 compositor.

use crate::bg::char_pixel_index;
use crate::memory::{unpack_rgb15, Memory};

/// SNES OBJ-per-scanline limit. Sprites beyond this many covering a line are
/// dropped in OAM-index order (lowest index kept).
pub const MAX_SPRITES_PER_LINE: usize = 32;

/// SNES per-line OBJ *time* (tile-fetch) limit: 34 8x8 tile-slivers.
pub const MAX_TILES_PER_LINE: usize = 34;

/// The OAMADD-derived evaluation start sprite. OAMADD ($2102 + $2103 bit 0) is a
/// 9-bit OAM *word* address; the low OAM table stores 2 words (4 bytes) per
/// sprite, so the priority-rotation start sprite is `(oam_addr >> 1) & 0x7f`
/// (wraps within the 128-sprite table).
pub fn obj_first_sprite(oam_addr: u16) -> usize {
    (oam_addr >> 1) as usize & 0x7f
}

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
pub(crate) fn sprite_dims(size_sel: u8, large: bool) -> (u32, u32) {
    OBJ_SIZE_PAIRS[(size_sel & 7) as usize][large as usize]
}

/// Per-scanline OAM evaluation result. `sprites` are the kept OBJ indices in
/// EVALUATION order (priority rotation applied), already trimmed by BOTH caps
/// (<=32 sprites, <=34 tile-slivers). The counts/flags feed `$213E` STAT77.
///
/// Off-screen-X rule (documented simplification, per fullsnes): the range/time
/// test is Y-ONLY. A sprite whose Y covers the line consumes a range slot and
/// its tile-slivers even when it is fully off-screen horizontally; the renderer
/// clips it per-pixel. Time-over drops WHOLE sprites (we do not model partial
/// tile fetches straddling the 34-tile budget).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LineBin {
    /// Kept + rendered sprites, in evaluation order.
    pub sprites: Vec<usize>,
    /// Total in-range sprites (Y test) this line, before the 32 cap (diagnostic).
    pub sprite_count: u16,
    /// Attempted tile-slivers among the range-kept (<=32) sprites (diagnostic).
    pub tile_count: u16,
    /// >32 sprites covered this line (STAT77 bit 6).
    pub range_over: bool,
    /// >34 tile-slivers among the range-kept sprites (STAT77 bit 7).
    pub time_over: bool,
}

/// Evaluate OAM for scanline `y`: apply priority-rotation eval order and BOTH
/// per-line caps, returning the kept set plus the STAT77 diagnostic counts.
///
/// Two-pass, matching the hardware pipeline:
///   1. Range: scan all 128 sprites in eval order, Y-test; count in-range and
///      keep the first 32. `range_over` when more than 32 were in range.
///   2. Time: walk the range-kept in eval order summing `w/8` slivers; keep the
///      eval-order prefix that fits within 34 (whole-sprite drop). `time_over`
///      when the range-kept attempted more than 34 slivers.
pub fn bin_line(mem: &Memory, y: usize) -> LineBin {
    let yi = y as i64;
    let size_sel = mem.obsel.size_sel;
    let start = if mem.priority_rotate {
        obj_first_sprite(mem.oam_addr)
    } else {
        0
    };

    // Pass 1 — range (Y-only), eval order, cap at 32.
    let mut in_range: Vec<usize> = Vec::with_capacity(MAX_SPRITES_PER_LINE);
    let mut sprite_count = 0u16;
    for k in 0..128usize {
        let i = (start + k) & 0x7f;
        let o = &mem.oam[i];
        if !o.on {
            continue;
        }
        let (_w, h) = sprite_dims(size_sel, o.large);
        let top = o.y as i64;
        if yi < top || yi >= top + h as i64 {
            continue;
        }
        sprite_count += 1;
        if in_range.len() < MAX_SPRITES_PER_LINE {
            in_range.push(i);
        }
    }
    let range_over = sprite_count as usize > MAX_SPRITES_PER_LINE;

    // Pass 2 — time (tile-fetch), eval order, keep prefix fitting in 34 slivers.
    let mut sprites: Vec<usize> = Vec::with_capacity(in_range.len());
    let mut tile_count = 0u16;
    let mut kept_tiles = 0usize;
    let mut budget_hit = false;
    for &i in &in_range {
        let (w, _h) = sprite_dims(size_sel, mem.oam[i].large);
        let slivers = (w / 8) as usize;
        tile_count += slivers as u16;
        if !budget_hit {
            if kept_tiles + slivers > MAX_TILES_PER_LINE {
                budget_hit = true; // this sprite + all after are time-dropped
            } else {
                kept_tiles += slivers;
                sprites.push(i);
            }
        }
    }
    let time_over = tile_count as usize > MAX_TILES_PER_LINE;

    LineBin {
        sprites,
        sprite_count,
        tile_count,
        range_over,
        time_over,
    }
}

/// OAM indices of the sprites kept + rendered on scanline `y`, in evaluation
/// order. Thin wrapper over [`bin_line`] for callers that only need the set.
pub fn sprites_on_line(mem: &Memory, y: usize) -> Vec<usize> {
    bin_line(mem, y).sprites
}

/// One OBJ tile is 4bpp: 16 VRAM words. Name-table addressing (16-wide row wrap,
/// row step, second-table gap) lives in [`obj_tile_addr`] below.
const OBJ_WORDS_PER_TILE: u32 = 16;

/// VRAM word address of the OBJ tile at block offset (`col`, `row`) from the
/// sprite's base `tile`, in the 16-wide OBJ name table. Right (+col) wraps within
/// the tile's own 16-tile row (`(name & 0x1f0) | ((name + col) & 0xf)`); down
/// (+row) steps +16, wrapping the 9-bit name. Names >= 256 live in the second
/// name table at `char_base + (name_select + 1) * 0x1000` (word-addressed).
pub(crate) fn obj_tile_addr(char_base: u32, name_select: u32, tile: u16, col: u32, row: u32) -> u16 {
    let row_tile = (tile as u32 + row * 16) & 0x1ff;
    let name = (row_tile & 0x1f0) | ((row_tile + col) & 0x0f);
    let addr = if name < 256 {
        char_base + name * OBJ_WORDS_PER_TILE
    } else {
        char_base + (name_select + 1) * 0x1000 + (name - 256) * OBJ_WORDS_PER_TILE
    };
    (addr & 0x7fff) as u16
}

/// Composite the given `indices` (already binned + capped, in evaluation order)
/// for scanline `y` into a `width`-long row of `Option<SpritePixel>`. First index
/// to paint a pixel wins it (eval-order priority). See [`render_scanline`] for the
/// pixel-sampling details.
pub fn render_scanline_for(
    mem: &Memory,
    indices: &[usize],
    y: usize,
    width: usize,
) -> Vec<Option<SpritePixel>> {
    let mut out = vec![None; width];
    let char_base = mem.obsel.char_base as u32;
    let size_sel = mem.obsel.size_sel;
    let name_select = mem.obsel.name_select as u32;
    for &i in indices {
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

/// Composite every kept sprite for scanline `y` (self-binning convenience for the
/// full-frame helper + unit tests). Real pixel sampling: per covering sprite (in
/// eval order), fetch its 4bpp char from the OBSEL char base in VRAM, apply
/// flip/size, decode the palette index, map index 0 = transparent / else
/// `cgram[128 + pal*16 + index]`. `SpritePixel.prio` carries the sprite priority.
pub fn render_scanline(mem: &Memory, y: usize, width: usize) -> Vec<Option<SpritePixel>> {
    render_scanline_for(mem, &bin_line(mem, y).sprites, y, width)
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
    fn obj_first_sprite_decodes_oamadd_word_address() {
        // OAMADD is a 9-bit OAM *word* address; the low table stores 2 words per
        // sprite, so the start sprite is (oam_addr >> 1) & 0x7f.
        assert_eq!(obj_first_sprite(0x000), 0); // word 0   -> sprite 0
        assert_eq!(obj_first_sprite(0x001), 0); // word 1   -> still sprite 0
        assert_eq!(obj_first_sprite(0x002), 1); // word 2   -> sprite 1
        assert_eq!(obj_first_sprite(0x00a), 5); // word 10  -> sprite 5
        assert_eq!(obj_first_sprite(0x0fe), 0x7f); // word 254 -> sprite 127
        assert_eq!(obj_first_sprite(0x100), 0); // word 256 -> (128 & 0x7f) = 0 (wrap)
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

    #[test]
    fn bin_line_range_cap_keeps_first_32_and_flags_over() {
        let mut mem = mem();
        for i in 0..40usize {
            mem.oam[i] = Obj { on: true, x: 0, y: 0, large: false, ..Obj::default() };
        }
        let bin = bin_line(&mem, 0);
        assert_eq!(bin.sprites.len(), MAX_SPRITES_PER_LINE);
        assert_eq!(bin.sprites.first(), Some(&0));
        assert_eq!(bin.sprites.last(), Some(&(MAX_SPRITES_PER_LINE - 1)));
        assert_eq!(bin.sprite_count, 40); // all in-range counted for the diagnostic
        assert!(bin.range_over);
        assert!(!bin.time_over); // 32 * 1 sliver = 32 tiles, under 34
    }

    #[test]
    fn bin_line_time_cap_drops_whole_sprites_and_flags_over() {
        // Size sel 2 large = 64x64 -> 8 slivers each. 5 sprites = 40 slivers > 34.
        let mut mem = mem();
        mem.obsel.size_sel = 2;
        for i in 0..5usize {
            mem.oam[i] = Obj { on: true, x: 0, y: 0, large: true, ..Obj::default() };
        }
        let bin = bin_line(&mem, 0);
        // 4 sprites * 8 = 32 tiles fit; the 5th (would be 40) is dropped whole.
        assert_eq!(bin.sprites, vec![0, 1, 2, 3]);
        assert_eq!(bin.sprite_count, 5); // range-side: all 5 are in range
        assert_eq!(bin.tile_count, 40); // attempted slivers among the range-kept
        assert!(bin.time_over);
        assert!(!bin.range_over);
    }

    #[test]
    fn bin_line_rotation_changes_eval_order_and_dropped_sprite() {
        // 33 8x8 sprites on the line: 32 kept, 1 dropped. Which one is dropped
        // depends on eval order. Rotation start = sprite 1 -> eval order is
        // 1,2,...,32,0 (wraps); the 33rd evaluated (index 0) is the drop.
        let mut mem = mem();
        for i in 0..33usize {
            mem.oam[i] = Obj { on: true, x: 0, y: 0, large: false, ..Obj::default() };
        }
        mem.priority_rotate = true;
        mem.oam_addr = 2; // obj_first_sprite(2) = 1
        let bin = bin_line(&mem, 0);
        assert_eq!(bin.sprites.len(), 32);
        assert_eq!(bin.sprites.first(), Some(&1)); // eval starts at 1
        assert!(!bin.sprites.contains(&0)); // index 0 is the wrapped-last drop
        assert!(bin.range_over);
        // Rotation OFF drops index 32 instead (ascending eval).
        mem.priority_rotate = false;
        let bin = bin_line(&mem, 0);
        assert!(bin.sprites.contains(&0));
        assert!(!bin.sprites.contains(&32));
    }

    #[test]
    fn bin_line_offscreen_x_sprite_still_counts_toward_caps() {
        // Documented rule (fullsnes): the range/time test is Y-ONLY. A sprite that
        // is fully off-screen horizontally (X < -w or X >= 256) but whose Y covers
        // the line still consumes a range slot and its tile-slivers.
        let mut mem = mem();
        mem.oam[0] = Obj { on: true, x: 300, y: 0, large: false, ..Obj::default() }; // off right
        mem.oam[1] = Obj { on: true, x: -300, y: 0, large: false, ..Obj::default() }; // off left
        let bin = bin_line(&mem, 0);
        assert_eq!(bin.sprite_count, 2);
        assert_eq!(bin.tile_count, 2);
        assert_eq!(bin.sprites, vec![0, 1]); // both kept in the set (renderer clips X)
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

    #[test]
    fn render_scanline_for_paints_only_given_indices_in_order() {
        let mut mem = obj_mem();
        mem.cgram[128 + 1] = rgb15(255, 0, 0);
        mem.cgram[128 + 2] = rgb15(0, 0, 255);
        let mut g1 = [[0u8; 8]; 8];
        g1[0][0] = 1;
        let mut g2 = [[0u8; 8]; 8];
        g2[0][0] = 2;
        put_obj_char(&mut mem, 1, g1);
        put_obj_char(&mut mem, 2, g2);
        mem.oam[0] = Obj { on: true, x: 0, y: 0, tile: 1, ..Obj::default() };
        mem.oam[5] = Obj { on: true, x: 0, y: 0, tile: 2, ..Obj::default() };
        // Explicit index order [5, 0]: sprite 5 is painted first -> wins the pixel.
        let line = render_scanline_for(&mem, &[5, 0], 0, crate::WIDTH);
        assert_eq!(line[0].unwrap().rgba, unpack_rgb15(rgb15(0, 0, 255)));
        // Empty set -> nothing painted.
        assert!(render_scanline_for(&mem, &[], 0, crate::WIDTH)[0].is_none());
    }
}
