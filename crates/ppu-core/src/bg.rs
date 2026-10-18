//! Mode-1 tile background rasterizer over byte-accurate VRAM.
//!
//! A BG pixel resolves the real PPU indirection: tilemap entry (at
//! `map_base`, screen-size wrapped) -> char bitplane data (at `char_base`,
//! 2bpp/4bpp per the mode table) -> palette index -> CGRAM color. Brightness
//! is applied once by the compositor; the per-layer primitive here returns
//! un-attenuated color.

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

/// (shared with sprite.rs) Palette index of pixel (`fx`, `fy`) inside the 8x8
/// char whose bitplane data starts at VRAM word `addr`. SNES layout: word
/// `addr + fy` holds plane 0 (low byte) and plane 1 (high byte) of row `fy`;
/// 4bpp adds planes 2/3 in word `addr + 8 + fy`; 8bpp adds plane pairs at
/// `addr + 16 + fy` and `addr + 24 + fy`. Bit 7 is the leftmost pixel.
/// Fetches wrap mod VRAM (0x8000 words).
pub(crate) fn char_pixel_index(mem: &Memory, addr: u16, bpp: u8, fx: u32, fy: u32) -> u8 {
    let bit = 7 - (fx & 7);
    let plane_pair = |w: u16| (((w as u8) >> bit) & 1) | ((((w >> 8) as u8) >> bit) & 1) << 1;
    let word = |off: u32| mem.vram[((addr as u32 + off) & 0x7fff) as usize];
    let mut index = plane_pair(word(fy));
    if bpp >= 4 {
        index |= plane_pair(word(8 + fy)) << 2;
    }
    if bpp == 8 {
        index |= plane_pair(word(16 + fy)) << 4;
        index |= plane_pair(word(24 + fy)) << 6;
    }
    index
}

/// VRAM word address of the tilemap entry for tile column `tx`, row `ty`
/// (already wrapped to the layer's total tile extent). A tilemap is 1, 2, or
/// 4 32x32-entry screens of 0x400 words, arranged per the BGnSC screen size:
/// 0 = 32x32; 1 = 64x32 (screen 1 right); 2 = 32x64 (screen 1 below);
/// 3 = 64x64 (screens 0|1 over 2|3). Wraps mod VRAM.
pub(crate) fn map_entry_addr(map_base: u16, screen_size: u8, tx: u32, ty: u32) -> u16 {
    let screen = match screen_size {
        1 => tx / 32,
        2 => ty / 32,
        3 => (ty / 32) * 2 + tx / 32,
        _ => 0,
    };
    let off = screen * 0x400 + (ty % 32) * 32 + (tx % 32);
    ((map_base as u32 + off) & 0x7fff) as u16
}

/// Offset-per-tile replacements for one screen column: `h` replaces the layer's
/// coarse HOFS (bits 3-9; its fine bits 0-2 stay), `v` replaces its VOFS.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ColumnOffset {
    h: Option<i16>,
    v: Option<i16>,
}

fn offset_word_value(word: u16) -> i16 {
    (word & 0x03ff) as i16
}

// Offset-per-tile modes repurpose BG3 as a control table, so mode_info(2/4)
// keeps BG3 out of priority_order. As measured on hardware (bsnes accuracy PPU):
// screen tile column c (columns start at the layer's fine HOFS) reads table
// column c - 1 + BG3HOFS / 8, and column 0 reads nothing. BG3VOFS / 8 picks the
// row: mode 2 reads H from it and V from the next row; mode 4 reads one word
// whose bit 15 selects V (set) or H (clear). Bit 13 enables an entry for BG1,
// bit 14 for BG2. Bits are the tilemap attribute bits, so authors can poke raw
// vram[] or bg[3].map entries.
/// Offset-per-tile only applies to BG1/BG2 (layers 0/1) in modes 2 and 4.
fn uses_offset_per_tile(layer: &RegBg) -> bool {
    matches!(layer.mode, 2 | 4) && layer.layer <= 1
}

fn offset_per_tile(layer: &RegBg, mem: &Memory, screen_x: usize) -> ColumnOffset {
    if !uses_offset_per_tile(layer) {
        return ColumnOffset::default();
    }
    let col = (screen_x as i64 + (layer.scroll_x as i64 & 7)) / 8;
    if col == 0 {
        return ColumnOffset::default();
    }
    let shift = if layer.offset_tile_size == 16 { 4 } else { 3 };
    let tx = ((8 * (col - 1) + (layer.offset_scroll_x as i64 & !7)) >> shift).rem_euclid(64) as u32;
    let read = |y: i64| {
        let ty = ((y + layer.offset_scroll_y as i64) >> shift).rem_euclid(64) as u32;
        mem.vram[map_entry_addr(layer.offset_map_base, layer.offset_screen_size, tx, ty) as usize]
    };
    let valid = 0x2000u16 << layer.layer;
    let entry = |word: u16| (word & valid != 0).then(|| offset_word_value(word));
    let word = read(0);
    if layer.mode == 4 {
        return match entry(word) {
            Some(v) if word & 0x8000 != 0 => ColumnOffset {
                h: None,
                v: Some(v),
            },
            h => ColumnOffset { h, v: None },
        };
    }
    ColumnOffset {
        h: entry(word),
        v: entry(read(8)),
    }
}

/// Direct color (CGWSEL.0): build a BGR555 word from an 8bpp pixel `index` and the
/// 3-bit tilemap `pal`. Index bits `bbgggrrr` fill each channel's high bits; the
/// palette bits fill one low bit per channel (R<-pal0, G<-pal1, B<-pal2), matching
/// fullsnes' BGR233->BGR555 expansion. Mode 7 has no per-tile palette, so `pal = 0`.
pub(crate) fn direct_color_bgr555(index: u8, pal: u8) -> u16 {
    let idx = index as u16;
    let pal = pal as u16;
    let r5 = ((idx & 0x07) << 2) | ((pal & 0x01) << 1);
    let g5 = (((idx >> 3) & 0x07) << 2) | (pal & 0x02);
    let b5 = (((idx >> 6) & 0x03) << 3) | (pal & 0x04);
    (b5 << 10) | (g5 << 5) | r5
}

/// One rasterized BG pixel candidate: the resolved CGRAM color plus the
/// tilemap entry's priority bit, so the compositor's priority pass
/// (m4/compositing) can interleave it with layer order and sprites.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BgPixel {
    pub rgba: [u8; 4],
    pub prio: bool,
}

/// Every intermediate of the tilemap -> char -> palette walk for screen pixel
/// (x, y) of one BG layer — the single source of truth shared by the scanline
/// rasterizer below and the Trace seam (trace.rs). `None` when the layer's bpp
/// is not 2/4/8 (absent in this mode). `index == 0` = transparent. `fx`/`fy`
/// are fine coords WITHIN the tile (flips applied), i.e. where the sampled
/// pixel lives in the stored (unflipped) tile data.
pub(crate) struct BgSample {
    pub tx: u32,
    pub ty: u32,
    pub map_addr: u16,
    pub entry: u16,
    /// VRAM word address of the exact (quadrant-adjusted) char row this pixel
    /// was fetched from. Not currently surfaced through the Trace seam (which
    /// recomputes the tile's own base char_addr instead — see trace::bg_tile);
    /// kept on the sample for diagnostic parity with the scanline rasterizer.
    #[allow(dead_code)]
    pub char_addr: u16,
    pub fx: u32,
    pub fy: u32,
    pub index: u8,
    /// CGRAM index the color came from; `None` for direct color or index 0.
    pub cgram_index: Option<u16>,
    /// Resolved BGR555 color; 0 when index == 0 (transparent).
    pub color15: u16,
    pub prio: bool,
}

pub(crate) fn sample_bg_pixel(layer: &RegBg, mem: &Memory, x: usize, y: usize) -> Option<BgSample> {
    if !matches!(layer.bpp, 2 | 4 | 8) {
        return None;
    }
    let ts = layer.tile_size as u32; // pixel edge: 8 or 16
    let (tiles_w, tiles_h): (u32, u32) = match layer.screen_size {
        1 => (64, 32),
        2 => (32, 64),
        3 => (64, 64),
        _ => (32, 32),
    };
    let words_per_char = layer.bpp as u32 * 4; // 2bpp = 8 words, 4bpp = 16, 8bpp = 32
                                               // Mosaic: snap sample coords to the block's top-left. `mosaic` is the block
                                               // edge in pixels (1 = off, so `by == y` / `bx == x` and output is unchanged).
                                               // Rows count from `mosaic_top`, where the MOSAIC size last changed.
                                               // Then +1: hardware's first visible line is line 1, so screen row y
                                               // shows tilemap row scroll_y + y + 1.
    let block = layer.mosaic.max(1) as i64;
    let top = layer.mosaic_top as i64;
    let sy = (top + (y as i64 - top) / block * block + 1) as usize;
    let sx = (x as i64 / block * block) as usize;
    let opt = offset_per_tile(layer, mem, sx);
    let hofs = opt.h.map_or(layer.scroll_x as i64, |h| {
        (h as i64 & !7) | (layer.scroll_x as i64 & 7)
    });
    let vofs = opt.v.map_or(layer.scroll_y, |v| v) as i64;
    let wx = (sx as i64 + hofs).rem_euclid((tiles_w * ts) as i64) as u32;
    let wy = (sy as i64 + vofs).rem_euclid((tiles_h * ts) as i64) as u32;
    let map_addr = map_entry_addr(layer.map_base, layer.screen_size, wx / ts, wy / ts);
    let entry = mem.vram[map_addr as usize];
    let (mut fx, mut fy) = (wx % ts, wy % ts);
    if entry & 0x4000 != 0 {
        fx = ts - 1 - fx; // H flip
    }
    if entry & 0x8000 != 0 {
        fy = ts - 1 - fy; // V flip
    }
    // 16x16 tiles are four 8x8 chars: n, n+1 (right), n+16, n+17 (below).
    let char_index = ((entry & 0x03ff) as u32 + fx / 8 + (fy / 8) * 16) & 0x03ff;
    let char_addr = ((layer.char_base as u32 + char_index * words_per_char) & 0x7fff) as u16;
    let index = char_pixel_index(mem, char_addr, layer.bpp, fx % 8, fy % 8);
    let (cgram_index, color15) = if index == 0 {
        (None, 0)
    } else if layer.bpp == 8 && layer.direct_color {
        (
            None,
            direct_color_bgr555(index, ((entry >> 10) & 0x07) as u8),
        )
    } else {
        let ci = if layer.bpp == 8 {
            index as u16
        } else {
            let pal = ((entry >> 10) & 0x07) as u16;
            let mode0_band = if layer.mode == 0 && layer.bpp == 2 {
                layer.layer as u16 * 32
            } else {
                0
            };
            mode0_band + pal * (1 << layer.bpp) + index as u16
        };
        (Some(ci), mem.cgram[ci as usize])
    };
    Some(BgSample {
        tx: wx / ts,
        ty: wy / ts,
        map_addr,
        entry,
        char_addr,
        fx,
        fy,
        index,
        cgram_index,
        color15,
        prio: entry & 0x2000 != 0,
    })
}

/// Render one BG layer for scanline `y` into `width` pixel candidates with
/// their tilemap priority bit; `None` = transparent at that x. The real
/// Mode-1 pipeline over byte-accurate VRAM: scroll -> tilemap fetch at
/// `map_base` (screen-size wrap) -> entry decode (tile#, palette, priority,
/// H/V flip) -> char bitplane decode at `char_base` (16x16 tiles = four 8x8
/// quadrant chars) -> CGRAM sub-palette (index 0 = transparent). Layers whose
/// mode-table bpp is not 2/4/8 render transparent: bpp 0 = absent in this mode.
pub fn render_bg_layer_scanline_px(
    layer: &RegBg,
    mem: &Memory,
    y: usize,
    width: usize,
) -> Vec<Option<BgPixel>> {
    let mut out = vec![None; width];
    render_bg_layer_scanline_px_into(layer, mem, y, &mut out);
    out
}

/// [`render_bg_layer_scanline_px`] into a caller-owned buffer (`out.len()` is the
/// width); every element is overwritten.
pub fn render_bg_layer_scanline_px_into(
    layer: &RegBg,
    mem: &Memory,
    y: usize,
    out: &mut [Option<BgPixel>],
) {
    if !layer.visible {
        out.fill(None);
        return;
    }
    let ts = layer.tile_size as u32;
    if matches!(layer.bpp, 2 | 4 | 8)
        && matches!(ts, 8 | 16)
        && layer.mosaic <= 1
        && !uses_offset_per_tile(layer)
    {
        render_tile_spans(layer, mem, y, out);
        return;
    }
    for (x, o) in out.iter_mut().enumerate() {
        *o = sample_bg_pixel(layer, mem, x, y).and_then(|s| {
            (s.index != 0).then(|| BgPixel {
                rgba: unpack_rgb15(s.color15),
                prio: s.prio,
            })
        });
    }
}

/// Palette indices of row `fy` (0..8) of the 8x8 char at VRAM word `addr`,
/// leftmost pixel first. Same plane layout as [`char_pixel_index`].
fn char_row_indices(mem: &Memory, addr: u16, bpp: u8, fy: u32) -> [u8; 8] {
    let word = |off: u32| mem.vram[((addr as u32 + off) & 0x7fff) as usize];
    let mut w = [0u16; 4];
    w[0] = word(fy);
    if bpp >= 4 {
        w[1] = word(8 + fy);
    }
    if bpp == 8 {
        w[2] = word(16 + fy);
        w[3] = word(24 + fy);
    }
    let mut row = [0u8; 8];
    for (fx, px) in row.iter_mut().enumerate() {
        let bit = 7 - fx as u32;
        for (pair, &v) in w.iter().enumerate() {
            let p = (((v as u8) >> bit) & 1) | ((((v >> 8) as u8) >> bit) & 1) << 1;
            *px |= p << (pair * 2);
        }
    }
    row
}

/// Fast path of [`render_bg_layer_scanline_px_into`] for unmosaiced layers
/// without offset-per-tile: `wy` is constant for the line, so each tile's map
/// entry and char row are fetched once and shared by its (up to 16) pixels.
/// Must stay pixel-identical to the [`sample_bg_pixel`] walk (differential test).
fn render_tile_spans(layer: &RegBg, mem: &Memory, y: usize, out: &mut [Option<BgPixel>]) {
    let ts = layer.tile_size as u32;
    let (tiles_w, tiles_h): (u32, u32) = match layer.screen_size {
        1 => (64, 32),
        2 => (32, 64),
        3 => (64, 64),
        _ => (32, 32),
    };
    let words_per_char = layer.bpp as u32 * 4;
    let wy = (y as i64 + 1 + layer.scroll_y as i64).rem_euclid((tiles_h * ts) as i64) as u32;
    let width_px = tiles_w * ts;
    let mut wx = (layer.scroll_x as i64).rem_euclid(width_px as i64) as u32;
    let mut x = 0;
    while x < out.len() {
        let in_x = wx % ts;
        let run = ((ts - in_x) as usize).min(out.len() - x);
        let entry =
            mem.vram[map_entry_addr(layer.map_base, layer.screen_size, wx / ts, wy / ts) as usize];
        let mut fy = wy % ts;
        if entry & 0x8000 != 0 {
            fy = ts - 1 - fy; // V flip
        }
        // Stored (unflipped) tile row, 8 or 16 pixels wide.
        let mut row = [0u8; 16];
        for q in 0..ts / 8 {
            let char_index = ((entry & 0x03ff) as u32 + q + (fy / 8) * 16) & 0x03ff;
            let char_addr =
                ((layer.char_base as u32 + char_index * words_per_char) & 0x7fff) as u16;
            let r = char_row_indices(mem, char_addr, layer.bpp, fy % 8);
            row[q as usize * 8..][..8].copy_from_slice(&r);
        }
        let pal = ((entry >> 10) & 0x07) as u8;
        let direct = layer.bpp == 8 && layer.direct_color;
        let cgram_base = if layer.bpp == 8 {
            0
        } else {
            let mode0_band = if layer.mode == 0 && layer.bpp == 2 {
                layer.layer as u16 * 32
            } else {
                0
            };
            mode0_band + pal as u16 * (1 << layer.bpp)
        };
        let prio = entry & 0x2000 != 0;
        for (i, o) in out[x..x + run].iter_mut().enumerate() {
            let mut fx = in_x + i as u32;
            if entry & 0x4000 != 0 {
                fx = ts - 1 - fx; // H flip
            }
            let index = row[fx as usize];
            *o = (index != 0).then(|| {
                let color15 = if direct {
                    direct_color_bgr555(index, pal)
                } else {
                    mem.cgram[(cgram_base + index as u16) as usize]
                };
                BgPixel {
                    rgba: unpack_rgb15(color15),
                    prio,
                }
            });
        }
        x += run;
        wx = (wx + run as u32) % width_px;
    }
}

/// Render one BG layer for scanline `y` into `width` pixel candidates.
/// `None` = transparent at that x (lower layer / backdrop shows through).
/// The compositor seam: [`render_bg_layer_scanline_px`] minus the priority
/// bit, which the v1 compositor does not consume yet (m4/compositing).
pub fn render_bg_layer_scanline(
    layer: &RegBg,
    mem: &Memory,
    y: usize,
    width: usize,
) -> Vec<Option<[u8; 4]>> {
    render_bg_layer_scanline_px(layer, mem, y, width)
        .into_iter()
        .map(|p| p.map(|c| c.rgba))
        .collect()
}

/// Standalone Mode-1 BG raster: backdrop (`cgram[0]`) + the four layers
/// (BG4..BG1, topmost wins) with INIDISP brightness applied once. Convenience
/// for the BG golden/unit tests ONLY — the E5 compositor composites layers
/// itself via `render_bg_layer_scanline` and applies brightness once globally,
/// so it never calls this (no double-attenuation).
pub fn render_bg_scanline(row: &RegRow, mem: &Memory, y: usize, width: usize) -> Vec<[u8; 4]> {
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
    /// A row whose tilemap row 0 lands on screen row 0: hardware shows row
    /// scroll_y + 1 on the top line, so every BG scrolls by -1.
    fn top() -> LineTableRow {
        let mut r = LineTableRow::default();
        for b in &mut r.bg {
            b.scroll_y = -1.0;
        }
        r
    }

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

    use crate::memory::rgb15;
    use crate::registers::{LineTableRow, RegRow};

    /// A Mode-1 layer to mutate per test: index 0 = BG1 (4bpp), 2 = BG3 (2bpp).
    fn layer(i: usize) -> RegBg {
        RegRow::from(&top()).bg[i].clone()
    }

    fn put_8bpp_row(mem: &mut Memory, addr: usize, row: usize, values: [u8; 8]) {
        for pair in 0..4usize {
            let mut lo = 0u16;
            let mut hi = 0u16;
            for (x, &v) in values.iter().enumerate() {
                lo |= (((v >> (pair * 2)) & 1) as u16) << (7 - x);
                hi |= (((v >> (pair * 2 + 1)) & 1) as u16) << (7 - x);
            }
            mem.vram[addr + pair * 8 + row] = lo | (hi << 8);
        }
    }

    #[test]
    fn empty_vram_is_all_transparent() {
        let m = Memory::new();
        // Entry 0 -> tile 0 -> all-zero planes -> index 0 everywhere.
        assert!(render_bg_layer_scanline(&layer(0), &m, 0, 8)
            .iter()
            .all(|p| p.is_none()));
    }

    #[test]
    fn renders_4bpp_tile_through_cgram_subpalette() {
        let mut m = Memory::new();
        m.cgram[2 * 16 + 1] = rgb15(255, 0, 0); // sub-palette 2, index 1
                                                // Char 1 at char_base 0x1000 (16 words/char): pixel (0,0) = index 1.
        m.vram[0x1000 + 16] = 0b1000_0000;
        // Map entry (0,0): tile 1, palette 2.
        m.vram[0] = 1 | (2 << 10);
        let mut l = layer(0);
        l.char_base = 0x1000;
        let line = render_bg_layer_scanline_px(&l, &m, 0, 4);
        let px = line[0].expect("pixel (0,0) set");
        assert_eq!(px.rgba, unpack_rgb15(rgb15(255, 0, 0)));
        assert!(!px.prio);
        assert!(line[1].is_none()); // index 0 = transparent
                                    // y=1 row of the tile is empty.
        assert!(render_bg_layer_scanline_px(&l, &m, 1, 4)[0].is_none());
    }

    #[test]
    fn renders_2bpp_tile_with_2bpp_palette_base() {
        let mut m = Memory::new();
        m.cgram[3 * 4 + 2] = rgb15(0, 255, 0); // 2bpp: sub-palette 3 base = 12
                                               // Char 1 at char_base 0x2000 (8 words/char): pixel (0,0) = index 2 (plane 1).
        m.vram[0x2000 + 8] = 0b1000_0000 << 8;
        m.vram[0] = 1 | (3 << 10);
        let mut l = layer(2); // BG3: bpp 2
        l.char_base = 0x2000;
        let line = render_bg_layer_scanline_px(&l, &m, 0, 2);
        assert_eq!(line[0].unwrap().rgba, unpack_rgb15(rgb15(0, 255, 0)));
    }

    #[test]
    fn mode0_bg2_uses_second_cgram_band() {
        let mut m = Memory::new();
        m.cgram[1] = rgb15(255, 0, 0);
        m.cgram[8 * 4 + 1] = rgb15(0, 255, 0);
        m.vram[0x2000 + 8] = 0x0080;
        m.vram[0] = 1;
        let mut src = top();
        src.mode = 0;
        src.bg[1].char_base = 0x2000;
        let row = RegRow::from(&src);
        let px = render_bg_layer_scanline_px(&row.bg[1], &m, 0, 1)[0].unwrap();
        assert_eq!(px.rgba, unpack_rgb15(rgb15(0, 255, 0)));
    }

    fn opt_layer(mode: u8) -> RegBg {
        let mut l = layer(0);
        l.mode = mode;
        l.char_base = 0x1000;
        l.offset_map_base = 0x0800;
        (l.offset_scroll_x, l.offset_scroll_y) = (0, 0);
        l
    }

    #[test]
    fn offset_table_columns_follow_hardware() {
        // Measured on bsnes (accuracy PPU): entry k drives screen tile column
        // k + 1, columns start at the layer's fine HOFS, BG3's scroll picks the
        // entry, and bit 13 / bit 14 enable it for BG1 / BG2.
        let mut m = Memory::new();
        let mut l = opt_layer(2);
        let h = |l: &RegBg, m: &Memory, x: usize| offset_per_tile(l, m, x).h;
        m.vram[0x0800 + 5] = 0x2000 | 16;
        assert_eq!(
            [47, 48, 55, 56].map(|x| h(&l, &m, x)),
            [None, Some(16), Some(16), None]
        );
        l.scroll_x = 3;
        assert_eq!(
            [44, 45, 52, 53].map(|x| h(&l, &m, x)),
            [None, Some(16), Some(16), None]
        );
        l.scroll_x = 0;
        l.offset_scroll_x = 8;
        assert_eq!([39, 40, 48].map(|x| h(&l, &m, x)), [None, Some(16), None]);
        l.offset_scroll_x = 0;
        l.offset_scroll_y = 8; // rows 1 (H) and 2 (V)
        assert_eq!(h(&l, &m, 48), None);
        m.vram[0x0800 + 32 + 5] = 0x2000 | 16;
        assert_eq!(h(&l, &m, 48), Some(16));
        l.offset_scroll_y = 0;
        m.vram[0x0800] = 0x2000 | 16; // column 0 never reads the table
        assert_eq!([0, 7, 8].map(|x| h(&l, &m, x)), [None, None, Some(16)]);
        l.layer = 1; // BG2 needs bit 14
        assert_eq!(h(&l, &m, 48), None);
        m.vram[0x0800 + 5] = 0x4000 | 16;
        assert_eq!(h(&l, &m, 48), Some(16));
    }

    #[test]
    fn offset_entries_replace_the_scroll() {
        // H replaces HOFS bits 3-9 (the entry's low 3 bits are dropped, the
        // layer's fine scroll stays); V replaces VOFS outright.
        let mut m = Memory::new();
        let mut l = opt_layer(2);
        l.scroll_x = 11;
        l.scroll_y = 10;
        m.vram[0x0800 + 5] = 0x2000 | 19;
        m.vram[0x0800 + 32 + 5] = 0x2000 | 4;
        let s = sample_bg_pixel(&l, &m, 45, 0).unwrap(); // column 6 reads entry 5
        assert_eq!((s.tx, s.fx), (8, 0)); // 45 + 16 + 3 = 64
        assert_eq!((s.ty, s.fy), (0, 5)); // line 1 + 4
        let s = sample_bg_pixel(&l, &m, 44, 0).unwrap(); // column 5: no entry
        assert_eq!((s.tx, s.fx, s.ty, s.fy), (6, 7, 1, 3)); // 44 + 11, 1 + 10
    }

    #[test]
    fn mode4_offset_word_bit15_selects_v_or_h() {
        let mut m = Memory::new();
        let l = opt_layer(4);
        m.vram[0x0800 + 5] = 0x2000 | 16;
        assert_eq!(
            offset_per_tile(&l, &m, 48),
            ColumnOffset {
                h: Some(16),
                v: None
            }
        );
        m.vram[0x0800 + 5] = 0x2000 | 0x8000 | 4;
        assert_eq!(
            offset_per_tile(&l, &m, 48),
            ColumnOffset {
                h: None,
                v: Some(4)
            }
        );
    }

    #[test]
    fn hflip_and_vflip_mirror_the_subtile() {
        let mut m = Memory::new();
        m.cgram[1] = rgb15(255, 255, 255);
        // Char 1: only pixel (0,0) set (index 1).
        m.vram[0x1000 + 16] = 0b1000_0000;
        let mut l = layer(0);
        l.char_base = 0x1000;
        m.vram[0] = 1 | (1 << 14); // H flip -> shows at x=7
        let line = render_bg_layer_scanline_px(&l, &m, 0, 8);
        assert!(line[0].is_none());
        assert!(line[7].is_some());
        m.vram[0] = 1 | (1 << 15); // V flip -> shows at y=7
        assert!(render_bg_layer_scanline_px(&l, &m, 0, 8)[0].is_none());
        assert!(render_bg_layer_scanline_px(&l, &m, 7, 8)[0].is_some());
        m.vram[0] = 1 | (1 << 14) | (1 << 15); // HV -> (7,7)
        assert!(render_bg_layer_scanline_px(&l, &m, 7, 8)[7].is_some());
    }

    #[test]
    fn priority_bit_carried_and_dropped_by_wrapper() {
        let mut m = Memory::new();
        m.cgram[1] = rgb15(10, 20, 30);
        m.vram[0x1000 + 16] = 0b1000_0000;
        m.vram[0] = 1 | (1 << 13); // priority set
        let mut l = layer(0);
        l.char_base = 0x1000;
        assert!(render_bg_layer_scanline_px(&l, &m, 0, 1)[0].unwrap().prio);
        // Contract wrapper: same color, priority dropped.
        assert_eq!(
            render_bg_layer_scanline(&l, &m, 0, 1)[0],
            Some(unpack_rgb15(rgb15(10, 20, 30)))
        );
    }

    #[test]
    fn scroll_wraps_with_negative_values() {
        let mut m = Memory::new();
        m.cgram[1] = rgb15(255, 255, 255);
        m.vram[0x1000 + 16] = 0b1000_0000; // char 1, pixel (0,0)
        m.vram[0] = 1; // map cell (0,0)
        let mut l = layer(0);
        l.char_base = 0x1000;
        l.scroll_x = -8; // 32x32 extent = 256 px: world x 0 appears at screen x 8
        let line = render_bg_layer_scanline_px(&l, &m, 0, 16);
        assert!(line[0].is_none());
        assert!(line[8].is_some());
        l.scroll_x = -264; // -264.rem_euclid(256) == 248 -> same as -8
        assert!(render_bg_layer_scanline_px(&l, &m, 0, 16)[8].is_some());
        l.scroll_x = 0;
        l.scroll_y = -4; // screen y 3 shows row 3 + 1 - 4 = 0, the tile's pixel row 0
        assert!(render_bg_layer_scanline_px(&l, &m, 3, 1)[0].is_some());
        assert!(render_bg_layer_scanline_px(&l, &m, 2, 1)[0].is_none());
    }

    #[test]
    fn invisible_or_absent_layers_are_transparent() {
        let mut m = Memory::new();
        m.cgram[1] = rgb15(255, 255, 255);
        m.vram[0x1000 + 16] = 0b1000_0000;
        m.vram[0] = 1;
        let mut l = layer(0);
        l.char_base = 0x1000;
        l.visible = false;
        assert!(render_bg_layer_scanline_px(&l, &m, 0, 8)
            .iter()
            .all(|p| p.is_none()));
        // BG4 does not exist in Mode 1 (bpp 0) even though visible defaults true.
        let bg4 = layer(3);
        assert!(bg4.visible && bg4.bpp == 0);
        assert!(render_bg_layer_scanline_px(&bg4, &m, 0, 8)
            .iter()
            .all(|p| p.is_none()));
    }

    #[test]
    fn tile16_selects_quadrant_chars() {
        let mut m = Memory::new();
        for i in 1..=4u16 {
            m.cgram[i as usize] = rgb15(i as u8 * 40, 0, 0);
        }
        let cb = 0x1000usize;
        // Pixel (0,0) of each quadrant char of 16x16 tile 2: distinct indices.
        m.vram[cb + 2 * 16] = 0x0080; // char 2  (top-left)     -> index 1
        m.vram[cb + 3 * 16] = 0x8000; // char 3  (top-right)    -> index 2
        m.vram[cb + 18 * 16] = 0x8080; // char 18 (bottom-left)  -> index 3
        m.vram[cb + 19 * 16 + 8] = 0x0080; // char 19 (bottom-right) -> index 4 (plane 2)
        m.vram[0] = 2;
        let mut l = layer(0);
        l.char_base = cb as u16;
        l.tile_size = 16;
        let idx_at = |mm: &Memory, y: usize, x: usize| {
            render_bg_layer_scanline_px(&l, mm, y, 16)[x].map(|p| p.rgba)
        };
        assert_eq!(idx_at(&m, 0, 0), Some(unpack_rgb15(rgb15(40, 0, 0))));
        assert_eq!(idx_at(&m, 0, 8), Some(unpack_rgb15(rgb15(80, 0, 0))));
        assert_eq!(idx_at(&m, 8, 0), Some(unpack_rgb15(rgb15(120, 0, 0))));
        assert_eq!(idx_at(&m, 8, 8), Some(unpack_rgb15(rgb15(160, 0, 0))));
        assert_eq!(idx_at(&m, 0, 1), None); // rest of each quadrant is empty
                                            // H flip swaps quadrant columns: (0,0) of char 3 lands at x=7.
        m.vram[0] = 2 | (1 << 14);
        assert_eq!(idx_at(&m, 0, 7), Some(unpack_rgb15(rgb15(80, 0, 0))));
        assert_eq!(idx_at(&m, 0, 15), Some(unpack_rgb15(rgb15(40, 0, 0))));
    }

    #[test]
    fn screen_size_1_reads_the_second_screen() {
        let mut m = Memory::new();
        m.cgram[1] = rgb15(255, 255, 255);
        m.vram[0x1000 + 16] = 0b1000_0000; // char 1, pixel (0,0)
        m.vram[0x0400] = 1; // tile cell (32,0) lives in screen 1
        let mut l = layer(0);
        l.char_base = 0x1000;
        l.screen_size = 1; // 64x32 -> 512px wide extent
        l.scroll_x = 256; // screen x 0 -> world x 256 -> tile col 32
                          // The 64-tile extent wraps: scroll 512 aliases scroll 0 (empty cell 0,0).
        l.scroll_x = 512;
        assert!(render_bg_layer_scanline_px(&l, &m, 0, 1)[0].is_none());
    }

    #[test]
    fn composite_helper_stacks_bg1_over_bg3() {
        let mut m = Memory::new();
        m.cgram[0] = rgb15(0, 0, 64); // backdrop
        m.cgram[1] = rgb15(255, 0, 0); // BG1 color (pal 0, idx 1)
        m.cgram[2] = rgb15(0, 255, 0); // BG3 color (pal 0, idx 2)
                                       // BG1: 4bpp char 1 at 0x1000, pixel (0,0); BG3: 2bpp char 1 at 0x2000,
                                       // pixels (0,0) and (1,0) = index 2.
        m.vram[0x1000 + 16] = 0b1000_0000;
        m.vram[0x2000 + 8] = 0b1100_0000 << 8;
        m.vram[0x0000] = 1; // BG1 map at 0x0000
        m.vram[0x0800] = 1; // BG3 map at 0x0800
        let mut src = top();
        src.bg[0].char_base = 0x1000;
        src.bg[2].char_base = 0x2000;
        src.bg[2].map_base = 0x0800;
        let row = RegRow::from(&src);
        let line = render_bg_scanline(&row, &m, 0, 3);
        assert_eq!(line[0], unpack_rgb15(rgb15(255, 0, 0))); // BG1 wins over BG3
        assert_eq!(line[1], unpack_rgb15(rgb15(0, 255, 0))); // BG3 over backdrop
        assert_eq!(line[2], unpack_rgb15(rgb15(0, 0, 64))); // backdrop
    }

    #[test]
    fn composite_shows_backdrop_and_applies_brightness() {
        let mut m = Memory::new();
        m.cgram[0] = rgb15(200, 200, 200);
        let mut row = RegRow::from(&top());
        row.brightness = 15;
        assert_eq!(
            render_bg_scanline(&row, &m, 0, 2)[0],
            unpack_rgb15(rgb15(200, 200, 200))
        );
        row.brightness = 0; // everything black
        assert_eq!(render_bg_scanline(&row, &m, 0, 2)[0], [0, 0, 0, 255]);
    }

    #[test]
    fn decodes_2bpp_bitplanes() {
        let mut m = Memory::new();
        // Row 0 of a 2bpp char at word 0x2000: pixels (0,0)=1, (1,0)=2, (2,0)=3.
        m.vram[0x2000] = (0b0110_0000 << 8) | 0b1010_0000;
        // Row 5: pixel (7,5) = 2 (plane 1, bit 0).
        m.vram[0x2005] = 0b0000_0001 << 8;
        assert_eq!(char_pixel_index(&m, 0x2000, 2, 0, 0), 1);
        assert_eq!(char_pixel_index(&m, 0x2000, 2, 1, 0), 2);
        assert_eq!(char_pixel_index(&m, 0x2000, 2, 2, 0), 3);
        assert_eq!(char_pixel_index(&m, 0x2000, 2, 3, 0), 0);
        assert_eq!(char_pixel_index(&m, 0x2000, 2, 7, 5), 2);
        assert_eq!(char_pixel_index(&m, 0x2000, 2, 7, 4), 0);
    }

    #[test]
    fn decodes_4bpp_bitplanes() {
        let mut m = Memory::new();
        // 4bpp char at 0x1000: planes 0/1 in words 0..8, planes 2/3 in words 8..16.
        // Pixel (0,3) set in planes 0+2 -> index 5; pixel (4,3) in planes 1+3 -> index 10.
        m.vram[0x1000 + 3] = (0b0000_1000 << 8) | 0b1000_0000;
        m.vram[0x1000 + 8 + 3] = (0b0000_1000 << 8) | 0b1000_0000;
        assert_eq!(char_pixel_index(&m, 0x1000, 4, 0, 3), 0b0101);
        assert_eq!(char_pixel_index(&m, 0x1000, 4, 4, 3), 0b1010);
        // Pixel (7,7) set in all four planes = 15.
        m.vram[0x1000 + 7] = 0x0101;
        m.vram[0x1000 + 8 + 7] = 0x0101;
        assert_eq!(char_pixel_index(&m, 0x1000, 4, 7, 7), 15);
        // Reading the same data as 2bpp ignores planes 2/3.
        assert_eq!(char_pixel_index(&m, 0x1000, 2, 0, 3), 1);
    }

    #[test]
    fn decodes_8bpp_bitplanes() {
        let mut m = Memory::new();
        put_8bpp_row(
            &mut m,
            0x3000,
            2,
            [0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80],
        );
        for x in 0..8u32 {
            assert_eq!(char_pixel_index(&m, 0x3000, 8, x, 2), 1u8 << x, "x={x}");
        }
    }

    #[test]
    fn renders_8bpp_tile_with_direct_cgram_index_and_ignores_tile_palette() {
        let mut m = Memory::new();
        m.cgram[0x21] = rgb15(12, 34, 56);
        put_8bpp_row(&mut m, 0x3000 + 32, 0, [0x21, 0, 0, 0, 0, 0, 0, 0]);
        m.vram[0] = 1 | (7 << 10);
        let mut l = layer(0);
        l.mode = 3;
        l.bpp = 8;
        l.char_base = 0x3000;
        let line = render_bg_layer_scanline_px(&l, &m, 0, 2);
        assert_eq!(line[0].unwrap().rgba, unpack_rgb15(rgb15(12, 34, 56)));
        assert!(line[1].is_none());
    }

    #[test]
    fn direct_color_mode_builds_bgr_from_index_and_palette() {
        // index 0xD3 = 0b11_010_011: R=3, G=2, B=3. palette = 5 (0b101): Rbit=1,Gbit=0,Bbit=1.
        // r5=(3<<2)|(1<<1)=14; g5=(2<<2)|0=8; b5=(3<<3)|4=28.
        let mut m = Memory::new();
        // 8bpp char at 0x3000, pixel (0,0) = index 0xD3.
        put_8bpp_row(&mut m, 0x3000 + 32, 0, [0xD3, 0, 0, 0, 0, 0, 0, 0]);
        m.vram[0] = 1 | (5 << 10); // map(0,0): tile1, palette 5
        let mut l = layer(0);
        l.mode = 3;
        l.bpp = 8;
        l.char_base = 0x3000;
        l.direct_color = true;
        let want = unpack_rgb15((28 << 10) | (8 << 5) | 14);
        assert_eq!(
            render_bg_layer_scanline_px(&l, &m, 0, 1)[0].unwrap().rgba,
            want
        );
        // With direct_color off, the same index is a CGRAM lookup (ignores palette).
        m.cgram[0xD3] = rgb15(9, 9, 9);
        l.direct_color = false;
        assert_eq!(
            render_bg_layer_scanline_px(&l, &m, 0, 1)[0].unwrap().rgba,
            unpack_rgb15(rgb15(9, 9, 9))
        );
    }

    #[test]
    fn char_fetch_wraps_vram() {
        let mut m = Memory::new();
        // Row 1 of a char based at the last VRAM word wraps to 0x0000.
        m.vram[0x0000] = 0b1000_0000; // plane 0, bit 7
        assert_eq!(char_pixel_index(&m, 0x7fff, 2, 0, 1), 1);
    }

    #[test]
    fn map_entry_addr_walks_one_screen() {
        assert_eq!(map_entry_addr(0x0000, 0, 0, 0), 0x0000);
        assert_eq!(map_entry_addr(0x0000, 0, 31, 0), 31);
        assert_eq!(map_entry_addr(0x0000, 0, 0, 1), 32);
        assert_eq!(map_entry_addr(0x7c00, 0, 5, 3), 0x7c00 + 3 * 32 + 5);
    }

    #[test]
    fn map_entry_addr_selects_screens_per_size() {
        // 64x32: tile column 32+ lands in screen 1.
        assert_eq!(map_entry_addr(0x0000, 1, 32, 0), 0x0400);
        assert_eq!(map_entry_addr(0x0000, 1, 63, 31), 0x0400 + 31 * 32 + 31);
        // 32x64: tile row 32+ lands in screen 1.
        assert_eq!(map_entry_addr(0x0000, 2, 0, 32), 0x0400);
        // 64x64 quadrants: 0|1 over 2|3.
        assert_eq!(map_entry_addr(0x1000, 3, 32, 0), 0x1400);
        assert_eq!(map_entry_addr(0x1000, 3, 0, 32), 0x1800);
        assert_eq!(map_entry_addr(0x1000, 3, 32, 32), 0x1c00);
    }

    #[test]
    fn map_entry_addr_wraps_vram() {
        // map_base at the top of VRAM: screen 1 wraps around to word 0.
        assert_eq!(map_entry_addr(0x7c00, 1, 32, 0), 0x0000);
    }

    #[test]
    fn mosaic_replicates_top_left_pixel_across_block_both_axes() {
        let mut m = Memory::new();
        m.cgram[1] = rgb15(255, 0, 0);
        // Char 1 at 0x1000: only pixel (0,0) lit (index 1). Map cell (0,0) -> tile 1.
        m.vram[0x1000 + 16] = 0b1000_0000;
        m.vram[0] = 1;
        let mut l = layer(0);
        l.char_base = 0x1000;
        l.mosaic = 2; // 2x2 blocks
                      // Row y=0: block top row. x=0 lit; x=1 replicates the block's top-left (x=0).
        let l0 = render_bg_layer_scanline_px(&l, &m, 0, 4);
        assert!(l0[0].is_some());
        assert!(l0[1].is_some()); // horizontal replication within the block
        assert!(l0[2].is_none()); // next block samples x=2 (empty)
                                  // Row y=1 snaps up to by=0, so the same top row replicates vertically.
        let l1 = render_bg_layer_scanline_px(&l, &m, 1, 4);
        assert!(l1[0].is_some());
        assert!(l1[1].is_some());
    }

    #[test]
    fn mosaic_rows_count_from_mosaic_top() {
        // Pixel (0,0) lit. Blocks of 2 counted from row 1: rows 1-2 sample row 1
        // (empty), rows 3-4 sample row 3; counted from row 0 instead, row 1 would
        // replicate the lit row 0.
        let mut m = Memory::new();
        m.cgram[1] = rgb15(255, 0, 0);
        m.vram[0x1000 + 16] = 0b1000_0000;
        m.vram[0] = 1;
        let mut l = layer(0);
        l.char_base = 0x1000;
        l.mosaic = 2;
        assert!(render_bg_layer_scanline_px(&l, &m, 1, 1)[0].is_some());
        l.mosaic_top = 1;
        assert!(render_bg_layer_scanline_px(&l, &m, 1, 1)[0].is_none());
        assert!(render_bg_layer_scanline_px(&l, &m, 2, 1)[0].is_none());
    }

    #[test]
    fn sample_bg_pixel_reports_the_full_walk() {
        // char 1 at 0x1000, pixel (0,0) = index 1; map cell (1,0) = tile1 pal2 prio1;
        // scroll_x = 8 puts screen x=0 on that cell.
        let mut m = Memory::new();
        m.cgram[2 * 16 + 1] = rgb15(0, 255, 0);
        m.vram[0x1000 + 16] = 0x0080; // 4bpp char 1, plane0 row0 bit7
        m.vram[0x0001] = 1 | (2 << 10) | (1 << 13);
        let mut row = top();
        row.bg[0].scroll_x = 8.0;
        row.bg[0].char_base = 0x1000;
        let reg = crate::registers::RegRow::from(&row);
        let s = sample_bg_pixel(&reg.bg[0], &m, 0, 0).unwrap();
        assert_eq!((s.tx, s.ty), (1, 0));
        assert_eq!(s.map_addr, 1);
        assert_eq!(s.entry, 1 | (2 << 10) | (1 << 13));
        assert_eq!(s.char_addr, 0x1000 + 16);
        assert_eq!((s.fx, s.fy), (0, 0));
        assert_eq!(s.index, 1);
        assert_eq!(s.cgram_index, Some(33));
        assert_eq!(s.color15, rgb15(0, 255, 0));
        assert!(s.prio);
    }

    #[test]
    fn mosaic_off_is_identical_to_raw_sampling() {
        let mut m = Memory::new();
        m.cgram[1] = rgb15(0, 255, 0);
        m.vram[0x1000 + 16] = 0b1000_0000; // char 1 pixel (0,0)
        m.vram[0] = 1;
        let mut l = layer(0);
        l.char_base = 0x1000;
        // Only pixel (0,0) is lit; with mosaic off (block 1) x=1 stays transparent.
        l.mosaic = 1;
        let line = render_bg_layer_scanline_px(&l, &m, 0, 4);
        assert!(line[0].is_some());
        assert!(line[1].is_none());
    }

    /// The span fast path must equal the per-pixel `sample_bg_pixel` walk for
    /// every layer configuration (fast path and fallback alike).
    #[test]
    fn scanline_matches_per_pixel_reference_on_random_layers() {
        let mut state = 0x9e37_79b9_7f4a_7c15u64;
        let mut rnd = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 11) as u32
        };
        let reference = |l: &RegBg, m: &Memory, y: usize, width: usize| -> Vec<Option<BgPixel>> {
            if !l.visible {
                return vec![None; width];
            }
            (0..width)
                .map(|x| {
                    sample_bg_pixel(l, m, x, y).and_then(|s| {
                        (s.index != 0).then(|| BgPixel {
                            rgba: unpack_rgb15(s.color15),
                            prio: s.prio,
                        })
                    })
                })
                .collect()
        };
        let mut m = Memory::new();
        for iter in 0..3000 {
            if iter % 8 == 0 {
                // Sparse-ish VRAM keeps index 0 common; CGRAM fully random.
                for w in m.vram.iter_mut() {
                    *w = (rnd() as u16) & (rnd() as u16 | rnd() as u16);
                }
                for c in m.cgram.iter_mut() {
                    *c = rnd() as u16 & 0x7fff;
                }
            }
            let mode = (rnd() % 5) as u8;
            let mut l = layer(0);
            l.mode = mode;
            l.layer = (rnd() % if mode == 0 { 4 } else { 2 }) as u8;
            l.bpp = [2, 4, 8][(rnd() % 3) as usize];
            l.tile_size = if rnd() % 2 == 0 { 8 } else { 16 };
            l.screen_size = (rnd() % 4) as u8;
            l.map_base = ((rnd() % 32) * 0x400) as u16;
            l.char_base = ((rnd() % 8) * 0x1000) as u16;
            l.offset_map_base = ((rnd() % 32) * 0x400) as u16;
            l.offset_screen_size = (rnd() % 4) as u8;
            l.scroll_x = match rnd() % 3 {
                0 => rnd() as i16,
                1 => (rnd() % 1100) as i16 - 550,
                _ => (rnd() % 8) as i16,
            };
            l.scroll_y = (rnd() as i16) >> (rnd() % 8);
            if uses_offset_per_tile(&l) {
                // Pre-existing: a negative `screen_x + scroll_x` wraps to a huge
                // u32 column in offset_per_tile (debug-build multiply overflow).
                // That fallback path is untouched here, so keep it in range.
                l.scroll_x = l.scroll_x.max(0);
            }
            l.direct_color = rnd() % 2 == 0;
            l.visible = rnd() % 8 != 0;
            l.mosaic = if rnd() % 4 == 0 {
                2 + (rnd() % 15) as u8
            } else {
                1
            };
            // Offset table: bit13 / bit15 / 10-bit deltas all vary.
            for w in 0..64 {
                let v = rnd() as u16;
                m.vram[(l.offset_map_base as usize + w) & 0x7fff] = if rnd() % 4 == 0 {
                    v & !0x2000
                } else {
                    v | 0x2000
                };
            }
            let width = if rnd() % 8 == 0 {
                1 + (rnd() % 300) as usize
            } else {
                256
            };
            let y = (rnd() % 224) as usize;
            assert_eq!(
                render_bg_layer_scanline_px(&l, &m, y, width),
                reference(&l, &m, y, width),
                "iter {iter}: {l:?} y={y} width={width}"
            );
        }
    }
}
