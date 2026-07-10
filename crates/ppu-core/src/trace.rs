//! Trace queries + single-layer isolation views (M9 inspector seams). Pure
//! read-only walks over a resolved `RegRow` + `Memory`, reusing the exact
//! rasterizer sample helpers (bg::sample_bg_pixel / mode7::mode7_sample /
//! sprite::obj_tile_addr) so the inspector shows what actually rendered — no
//! duplicate decode logic. Serde camelCase to match the TS seam types 1:1.

use serde::Serialize;

use crate::bg::{
    char_pixel_index, direct_color_bgr555, map_entry_addr, render_bg_layer_scanline_px,
    sample_bg_pixel,
};
use crate::linetable::LineTable;
use crate::memory::{unpack_rgb15, Memory};
use crate::mode7::{mode7_sample, render_mode7_scanline, render_mode7_scanline_px};
use crate::registers::{RegBg, RegRow};
use crate::sprite::{bin_line, obj_tile_addr, render_scanline_for, sprite_dims};
use crate::{OamSprite, HEIGHT, WIDTH};

/// Stage 1 of the chain: the source registers the selection resolves through.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BgTraceRegs {
    pub mode: u8,
    /// 1-based DSL layer number (bg[n]).
    pub layer: u8,
    pub map_base: u16,
    pub char_base: u16,
    pub tile_size: u8,
    pub screen_size: u8,
    pub bpp: u8,
    pub scroll_x: i16,
    pub scroll_y: i16,
    /// Effective mosaic block edge (1 = off).
    pub mosaic: u8,
    pub direct_color: bool,
    pub visible: bool,
}

/// Stage 2 + 3: the tilemap entry and the tile's stored (unflipped) pixel data.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BgTraceTile {
    pub tx: u32,
    pub ty: u32,
    /// VRAM word address of the map entry (Mode 7: the interleaved word).
    pub map_addr: u16,
    /// Raw entry word (Mode 7: the whole interleaved word).
    pub entry: u16,
    pub tile: u16,
    pub pal: u8,
    pub prio: bool,
    pub flip_x: bool,
    pub flip_y: bool,
    /// VRAM word address of the tile's first char row.
    pub char_addr: u16,
    /// tile_size x tile_size palette indices, row-major, unflipped (as stored).
    /// Mode 7: always 8x8.
    pub pixels: Vec<u8>,
    /// CGRAM base index of the tile's sub-palette (0 for 8bpp / direct color).
    pub palette_base: u16,
}

/// Stage 4: the selected pixel resolved to a color (screen-pixel selection only).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TracePixel {
    pub x: u32,
    pub y: u32,
    /// Fine coords within the tile's stored pixel grid (flips applied).
    pub fx: u32,
    pub fy: u32,
    pub index: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cgram_index: Option<u16>,
    pub bgr555: u16,
    pub rgb: [u8; 3],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BgTrace {
    pub regs: BgTraceRegs,
    pub tile: BgTraceTile,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pixel: Option<TracePixel>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjTrace {
    pub index: u8,
    pub oam: OamSprite,
    /// OBSEL char base (VRAM word address).
    pub char_base: u16,
    /// VRAM word address of the sprite's top-left char.
    pub char_addr: u16,
    pub width: u32,
    pub height: u32,
    /// width x height palette indices, row-major, unflipped (as stored);
    /// the oam flip flags say how it renders.
    pub pixels: Vec<u8>,
    /// CGRAM base index (128 + pal*16).
    pub palette_base: u16,
    /// The 16 BGR555 words of the sprite's sub-palette.
    pub palette: Vec<u16>,
}

/// Stage-1 regs for a tile-mode BG layer: a straight copy of the resolved
/// `RegBg`, plus the row's mode and the 1-based DSL layer number.
fn bg_regs(row: &RegRow, layer: usize) -> BgTraceRegs {
    let l = &row.bg[layer];
    BgTraceRegs {
        mode: row.mode,
        layer: layer as u8 + 1,
        map_base: l.map_base,
        char_base: l.char_base,
        tile_size: l.tile_size,
        screen_size: l.screen_size,
        bpp: l.bpp,
        scroll_x: l.scroll_x,
        scroll_y: l.scroll_y,
        mosaic: l.mosaic,
        direct_color: l.direct_color,
        visible: l.visible,
    }
}

/// Stage 2+3 for a tile-mode BG layer: decode the tilemap entry and walk the
/// SAME quadrant formula `sample_bg_pixel` uses to collect every stored pixel
/// of the (possibly 16x16) tile, unflipped.
fn bg_tile(l: &RegBg, mem: &Memory, entry: u16, map_addr: u16, tx: u32, ty: u32) -> BgTraceTile {
    let tile = entry & 0x03ff;
    let pal = ((entry >> 10) & 0x07) as u8;
    let prio = entry & 0x2000 != 0;
    let flip_x = entry & 0x4000 != 0;
    let flip_y = entry & 0x8000 != 0;
    let words_per_char = l.bpp as u32 * 4;
    let char_addr = ((l.char_base as u32 + tile as u32 * words_per_char) & 0x7fff) as u16;
    let ts = l.tile_size as u32;
    let mut pixels = Vec::with_capacity((ts * ts) as usize);
    for fy in 0..ts {
        for fx in 0..ts {
            let char_index = (tile as u32 + fx / 8 + (fy / 8) * 16) & 0x03ff;
            let addr = ((l.char_base as u32 + char_index * words_per_char) & 0x7fff) as u16;
            pixels.push(char_pixel_index(mem, addr, l.bpp, fx % 8, fy % 8));
        }
    }
    let mode0_band = if l.mode == 0 && l.bpp == 2 {
        l.layer as u16 * 32
    } else {
        0
    };
    let palette_base = if l.bpp == 8 {
        0
    } else {
        mode0_band + pal as u16 * (1 << l.bpp)
    };
    BgTraceTile {
        tx,
        ty,
        map_addr,
        entry,
        tile,
        pal,
        prio,
        flip_x,
        flip_y,
        char_addr,
        pixels,
        palette_base,
    }
}

/// Mode-7 stages 1+2+3+4 for screen pixel (x, y): mirrors the render color
/// rule of `render_mode7_scanline`/`render_mode7_scanline_px` EXACTLY so the
/// trace shows what actually rendered.
fn trace_mode7(row: &RegRow, mem: &Memory, x: usize, y: usize) -> BgTrace {
    let s = mode7_sample(row, mem, y, x);
    let color_idx = if row.extbg() { s.index & 0x7f } else { s.index };
    let (cgram_index, bgr555) = if color_idx == 0 {
        (None, 0u16)
    } else if row.bg[0].direct_color {
        (None, direct_color_bgr555(color_idx, 0))
    } else {
        (Some(color_idx as u16), mem.cgram[color_idx as usize])
    };
    let rgb = unpack_rgb15(bgr555);
    let entry = mem.vram[s.map_addr as usize];
    let tile = s.tile as u16;
    let prio = row.extbg() && s.index & 0x80 != 0;
    let char_addr = tile.wrapping_mul(64);
    let pixels = (0..64)
        .map(|i| (mem.vram[tile as usize * 64 + i] >> 8) as u8)
        .collect();
    BgTrace {
        regs: BgTraceRegs {
            mode: 7,
            layer: 1,
            map_base: 0,
            char_base: 0,
            tile_size: 8,
            screen_size: 0,
            bpp: 8,
            scroll_x: row.bg[0].scroll_x,
            scroll_y: row.bg[0].scroll_y,
            mosaic: row.bg[0].mosaic,
            direct_color: row.bg[0].direct_color,
            visible: row.bg[0].visible,
        },
        tile: BgTraceTile {
            tx: s.tx as u32,
            ty: s.ty as u32,
            map_addr: s.map_addr,
            entry,
            tile,
            pal: 0,
            prio,
            flip_x: false,
            flip_y: false,
            char_addr,
            pixels,
            palette_base: 0,
        },
        pixel: Some(TracePixel {
            x: x as u32,
            y: y as u32,
            fx: s.fx as u32,
            fy: s.fy as u32,
            index: s.index,
            cgram_index,
            bgr555,
            rgb: [rgb[0], rgb[1], rgb[2]],
        }),
    }
}

/// Trace a BG plane at screen pixel (x, y) using scanline y's resolved
/// registers. `layer` is 0-based here (bg[0..3]); the wasm shim converts from
/// the seam's 1-based number. `None` = the layer does not exist in this row's
/// mode. Works on hidden layers (visibility is reported, not enforced).
pub fn trace_bg_screen(
    row: &RegRow,
    mem: &Memory,
    layer: usize,
    x: usize,
    y: usize,
) -> Option<BgTrace> {
    if layer >= 4 {
        return None; // self-defending seam: never index outside bg[0..3]
    }
    if row.mode == 7 {
        if layer != 0 {
            return None;
        }
        return Some(trace_mode7(row, mem, x, y));
    }
    let s = sample_bg_pixel(&row.bg[layer], mem, x, y)?;
    let rgb = unpack_rgb15(s.color15);
    Some(BgTrace {
        regs: bg_regs(row, layer),
        tile: bg_tile(&row.bg[layer], mem, s.entry, s.map_addr, s.tx, s.ty),
        pixel: Some(TracePixel {
            x: x as u32,
            y: y as u32,
            fx: s.fx,
            fy: s.fy,
            index: s.index,
            cgram_index: s.cgram_index,
            bgr555: s.color15,
            rgb: [rgb[0], rgb[1], rgb[2]],
        }),
    })
}

/// Trace a BG plane at tilemap cell (tx, ty) — the Memory-grid selection.
/// The caller (wasm shim) picks the register row.
pub fn trace_bg_tile(
    row: &RegRow,
    mem: &Memory,
    layer: usize,
    tx: u32,
    ty: u32,
) -> Option<BgTrace> {
    if layer >= 4 {
        return None; // self-defending seam: never index outside bg[0..3]
    }
    if row.mode == 7 {
        if layer != 0 {
            return None;
        }
        let tx = tx & 127;
        let ty = ty & 127;
        let map_addr = (ty * 128 + tx) as u16;
        let entry = mem.vram[map_addr as usize];
        let tile = entry & 0x00ff;
        let char_addr = tile.wrapping_mul(64);
        let pixels = (0..64)
            .map(|i| (mem.vram[tile as usize * 64 + i] >> 8) as u8)
            .collect();
        return Some(BgTrace {
            regs: BgTraceRegs {
                mode: 7,
                layer: 1,
                map_base: 0,
                char_base: 0,
                tile_size: 8,
                screen_size: 0,
                bpp: 8,
                scroll_x: row.bg[0].scroll_x,
                scroll_y: row.bg[0].scroll_y,
                mosaic: row.bg[0].mosaic,
                direct_color: row.bg[0].direct_color,
                visible: row.bg[0].visible,
            },
            tile: BgTraceTile {
                tx,
                ty,
                map_addr,
                entry,
                tile,
                pal: 0,
                prio: false,
                flip_x: false,
                flip_y: false,
                char_addr,
                pixels,
                palette_base: 0,
            },
            pixel: None,
        });
    }
    let l = &row.bg[layer];
    if !matches!(l.bpp, 2 | 4 | 8) {
        return None;
    }
    let (tiles_w, tiles_h): (u32, u32) = match l.screen_size {
        1 => (64, 32),
        2 => (32, 64),
        3 => (64, 64),
        _ => (32, 32),
    };
    let tx = tx % tiles_w;
    let ty = ty % tiles_h;
    let map_addr = map_entry_addr(l.map_base, l.screen_size, tx, ty);
    let entry = mem.vram[map_addr as usize];
    Some(BgTrace {
        regs: bg_regs(row, layer),
        tile: bg_tile(l, mem, entry, map_addr, tx, ty),
        pixel: None,
    })
}

/// Trace an OAM sprite: OAM entry -> OBSEL char base -> char data -> palette.
pub fn trace_obj(mem: &Memory, index: usize) -> Option<ObjTrace> {
    if index >= 128 {
        return None;
    }
    let o = &mem.oam[index];
    let (w, h) = sprite_dims(mem.obsel.size_sel, o.large);
    let char_base = mem.obsel.char_base as u32;
    let name_select = mem.obsel.name_select as u32;
    let mut pixels = Vec::with_capacity((w * h) as usize);
    for py in 0..h {
        for px in 0..w {
            let addr = obj_tile_addr(char_base, name_select, o.tile, px / 8, py / 8);
            pixels.push(char_pixel_index(mem, addr, 4, px % 8, py % 8));
        }
    }
    let char_addr = obj_tile_addr(char_base, name_select, o.tile, 0, 0);
    let palette_base = 128 + (o.pal & 7) as u16 * 16;
    let palette = mem.cgram[palette_base as usize..palette_base as usize + 16].to_vec();
    Some(ObjTrace {
        index: index as u8,
        oam: OamSprite::from(o),
        char_base: mem.obsel.char_base,
        char_addr,
        width: w,
        height: h,
        pixels,
        palette_base,
        palette,
    })
}

/// Render ONE plane in isolation (256x224 RGBA, alpha 0 where the plane is
/// transparent). `plane` 0..3 = BG1..BG4, 4 = OBJ. Ignores TM/TS, windows and
/// priority; honors per-row mode, layer `visible`, mosaic and force-blank
/// (blank lines stay transparent). The Trace tab's source-stage minimap.
pub fn render_layer_view(lt: &LineTable, mem: &Memory, plane: u8) -> Vec<u8> {
    let mut fb = vec![0u8; WIDTH * HEIGHT * 4];
    for y in 0..lt.rows.len().min(HEIGHT) {
        let row = &lt.rows[y];
        if row.force_blank {
            continue;
        }
        let row_off = y * WIDTH * 4;
        match plane {
            4 => {
                let bin = bin_line(mem, y);
                for (x, sp) in render_scanline_for(mem, &bin.sprites, y, WIDTH)
                    .into_iter()
                    .enumerate()
                {
                    if let Some(s) = sp {
                        let o = row_off + x * 4;
                        fb[o..o + 4].copy_from_slice(&s.rgba);
                    }
                }
            }
            0..=3 if row.mode == 7 => {
                if plane == 0 && row.bg[0].visible {
                    if row.extbg() {
                        for (x, p) in render_mode7_scanline_px(row, mem, y, WIDTH)
                            .into_iter()
                            .enumerate()
                        {
                            if let Some(px) = p {
                                let o = row_off + x * 4;
                                fb[o..o + 4].copy_from_slice(&px.rgba);
                            }
                        }
                    } else {
                        let mut tmp = vec![0u8; WIDTH * 4];
                        render_mode7_scanline(row, mem, y, &mut tmp);
                        for x in 0..WIDTH {
                            if tmp[x * 4 + 3] != 0 {
                                let o = row_off + x * 4;
                                fb[o] = tmp[x * 4];
                                fb[o + 1] = tmp[x * 4 + 1];
                                fb[o + 2] = tmp[x * 4 + 2];
                                fb[o + 3] = 255;
                            }
                        }
                    }
                }
            }
            0..=3 => {
                for (x, p) in render_bg_layer_scanline_px(&row.bg[plane as usize], mem, y, WIDTH)
                    .into_iter()
                    .enumerate()
                {
                    if let Some(px) = p {
                        let o = row_off + x * 4;
                        fb[o..o + 4].copy_from_slice(&px.rgba);
                    }
                }
            }
            _ => {}
        }
    }
    fb
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linetable::LineTableBuilder;
    use crate::memory::rgb15;
    use crate::memory::unpack_rgb15;
    use crate::registers::{LineTableRow, Obj, RegRow};
    use crate::render_frame;

    /// BG1: char 1 at 0x1000 (pixel (0,0) = idx 1), map cell (1,0) = tile1 pal2
    /// prio1, scroll_x = 8 -> screen (0,0) lands on it.
    fn bg_scene() -> (Memory, RegRow) {
        let mut m = Memory::new();
        m.cgram[2 * 16 + 1] = rgb15(0, 255, 0);
        m.vram[0x1000 + 16] = 0x0080;
        m.vram[0x0001] = 1 | (2 << 10) | (1 << 13);
        let mut row = LineTableRow::default();
        row.bg[0].scroll_x = 8.0;
        row.bg[0].char_base = 0x1000;
        (m, RegRow::from(&row))
    }

    #[test]
    fn trace_bg_pixel_reports_the_full_chain() {
        let (m, row) = bg_scene();
        let t = trace_bg_screen(&row, &m, 0, 0, 0).unwrap();
        assert_eq!(t.regs.layer, 1);
        assert_eq!(t.regs.mode, 1);
        assert_eq!(t.regs.bpp, 4);
        assert_eq!(t.regs.char_base, 0x1000);
        assert_eq!(t.regs.scroll_x, 8);
        assert_eq!((t.tile.tx, t.tile.ty), (1, 0));
        assert_eq!(t.tile.map_addr, 1);
        assert_eq!(t.tile.tile, 1);
        assert_eq!(t.tile.pal, 2);
        assert!(t.tile.prio);
        assert_eq!(t.tile.char_addr, 0x1000 + 16);
        assert_eq!(t.tile.pixels.len(), 64);
        assert_eq!(t.tile.pixels[0], 1); // stored pixel (0,0)
        assert_eq!(t.tile.palette_base, 2 * 16);
        let p = t.pixel.unwrap();
        assert_eq!((p.fx, p.fy), (0, 0));
        assert_eq!(p.index, 1);
        assert_eq!(p.cgram_index, Some(33));
        assert_eq!(p.bgr555, rgb15(0, 255, 0));
        let c = unpack_rgb15(rgb15(0, 255, 0));
        assert_eq!(p.rgb, [c[0], c[1], c[2]]);
    }

    #[test]
    fn trace_pixel_color_matches_the_rendered_frame() {
        // Chain honesty: the traced bgr555 is exactly the framebuffer pixel
        // (brightness 15, no math in this scene).
        let (m, _) = bg_scene();
        let mut src = LineTableRow::default();
        src.bg[0].scroll_x = 8.0;
        src.bg[0].char_base = 0x1000;
        let lt = LineTableBuilder::new(src).build(HEIGHT);
        let fb = render_frame(&lt, &m);
        let t = trace_bg_screen(&lt.rows[0], &m, 0, 0, 0).unwrap();
        let px = unpack_rgb15(t.pixel.unwrap().bgr555);
        assert_eq!(&fb[0..4], &px);
    }

    #[test]
    fn trace_bg_absent_layer_returns_none_and_hidden_layer_still_traces() {
        let (m, mut row) = bg_scene();
        assert!(trace_bg_screen(&row, &m, 3, 0, 0).is_none()); // BG4 absent in mode 1
        row.bg[0].visible = false;
        let t = trace_bg_screen(&row, &m, 0, 0, 0).unwrap();
        assert!(!t.regs.visible); // reported, not enforced
        assert_eq!(t.pixel.unwrap().index, 1);
    }

    #[test]
    fn trace_bg_tile_selection_addresses_the_map_cell() {
        let (m, row) = bg_scene();
        let t = trace_bg_tile(&row, &m, 0, 1, 0).unwrap();
        assert_eq!(t.tile.map_addr, 1);
        assert_eq!(t.tile.tile, 1);
        assert!(t.pixel.is_none());
        // wraps by screen size (32x32): tx=33 -> tx=1
        let w = trace_bg_tile(&row, &m, 0, 33, 0).unwrap();
        assert_eq!(w.tile.map_addr, 1);
    }

    #[test]
    fn trace_mode7_pixel_chain() {
        let mut m = Memory::new();
        m.cgram[5] = rgb15(255, 0, 255);
        m.vram[0] = 5 << 8; // map tile 0; char (0,0) idx 5
        let mut src = LineTableRow::default();
        src.mode = 7;
        let row = RegRow::from(&src);
        let t = trace_bg_screen(&row, &m, 0, 0, 0).unwrap();
        assert_eq!(t.regs.mode, 7);
        assert_eq!(t.regs.bpp, 8);
        assert_eq!(t.tile.map_addr, 0);
        assert_eq!(t.tile.tile, 0);
        assert_eq!(t.tile.pixels.len(), 64);
        assert_eq!(t.tile.pixels[0], 5);
        let p = t.pixel.unwrap();
        assert_eq!(p.index, 5);
        assert_eq!(p.cgram_index, Some(5));
        assert_eq!(p.bgr555, rgb15(255, 0, 255));
        // other planes don't exist in mode 7
        assert!(trace_bg_screen(&row, &m, 1, 0, 0).is_none());
    }

    #[test]
    fn trace_obj_reports_dims_palette_and_stored_pixels() {
        let mut m = Memory::new();
        m.obsel.char_base = 0x4000;
        m.vram[0x4000 + 16] = 0x0080; // char 1 pixel (0,0) = 1
        m.cgram[128 + 5 * 16 + 1] = rgb15(255, 255, 0);
        m.oam[3] = Obj {
            on: true,
            x: 10,
            y: 20,
            tile: 1,
            pal: 5,
            prio: 2,
            ..Obj::default()
        };
        let t = trace_obj(&m, 3).unwrap();
        assert_eq!(t.index, 3);
        assert_eq!(t.oam.tile, 1);
        assert_eq!((t.width, t.height), (8, 8)); // size_sel 0 small
        assert_eq!(t.char_base, 0x4000);
        assert_eq!(t.char_addr, 0x4000 + 16);
        assert_eq!(t.pixels.len(), 64);
        assert_eq!(t.pixels[0], 1);
        assert_eq!(t.palette_base, 128 + 5 * 16);
        assert_eq!(t.palette[1], rgb15(255, 255, 0));
        assert!(trace_obj(&m, 128).is_none());
    }

    #[test]
    fn layer_view_isolates_one_plane() {
        let (m, _) = bg_scene();
        let mut src = LineTableRow::default();
        src.bg[0].scroll_x = 8.0;
        src.bg[0].char_base = 0x1000;
        // BG2 defaults char_base/map_base to 0, which would otherwise alias
        // BG1's tilemap word at VRAM address 1 (map_base 0, tx=1) as BG2's own
        // char data; move BG2's char fetch to untouched VRAM so it stays empty.
        // char_base snaps to 0x1000-word steps, so 0x2000 (not 0x0800) is the
        // nearest register-representable value away from the collision.
        src.bg[1].char_base = 0x2000;
        src.tm = 0x00; // masked off the main screen — the view must not care
        let lt = LineTableBuilder::new(src).build(HEIGHT);
        let v = render_layer_view(&lt, &m, 0);
        assert_eq!(v.len(), WIDTH * HEIGHT * 4);
        assert_eq!(v[3], 255); // (0,0) opaque: the traced tile pixel
        assert_eq!(&v[0..3], &unpack_rgb15(rgb15(0, 255, 0))[..3]);
        assert_eq!(v[7], 0); // (1,0) transparent
        let empty = render_layer_view(&lt, &m, 1); // BG2 has no content
        assert!(empty.chunks(4).all(|p| p[3] == 0));
    }

    #[test]
    fn layer_view_renders_the_obj_plane() {
        let mut m = Memory::new();
        m.obsel.char_base = 0x4000;
        m.vram[0x4000 + 16] = 0x0080;
        m.cgram[128 + 1] = rgb15(255, 255, 0);
        m.oam[0] = Obj {
            on: true,
            x: 0,
            y: 0,
            tile: 1,
            prio: 0,
            ..Obj::default()
        };
        let lt = LineTableBuilder::new(LineTableRow::default()).build(HEIGHT);
        let v = render_layer_view(&lt, &m, 4);
        assert_eq!(&v[0..4], &unpack_rgb15(rgb15(255, 255, 0)));
        assert_eq!(v[7], 0);
    }
}
