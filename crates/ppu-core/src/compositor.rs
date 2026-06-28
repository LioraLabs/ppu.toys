//! Phase-2 frame compositor. Ties the per-scanline rasterizers together into the
//! full 256x224 RGBA framebuffer from a resolved `LineTable` + `Memory`.
//!
//! Per scanline `y`:
//!   1. select the active mode from that row (per-line `mode` -> split-screen);
//!   2. start from the backdrop `unpack_rgb15(cgram[0])` (opaque);
//!   3. paint BG: Mode 7 floor (mode7.rs) when `row.mode == 7`, else the Mode-1
//!      tile layers BG4..BG1 (bg.rs), topmost non-transparent wins;
//!   4. overlay sprites (sprite.rs) on top (v1: sprites always above BG; sprite
//!      `prio` orders sprites among themselves, handled in `render_scanline`);
//!   5. apply INIDISP brightness ONCE to the final pixel (`apply_brightness`).
//!
//! Brightness single-application point: HERE. The scanline primitives this
//! compositor calls all return un-attenuated direct RGBA, so brightness is never
//! double-applied.

use crate::bg::{apply_brightness, render_bg_layer_scanline};
use crate::linetable::LineTable;
use crate::memory::{unpack_rgb15, Memory};
use crate::mode7::render_mode7_scanline;
use crate::registers::LineTableRow;
use crate::sprite::render_scanline as render_sprite_scanline;
use crate::{HEIGHT, WIDTH};

/// Composite one scanline `y` of `row` into `line` (length `WIDTH`), backdrop +
/// BG + sprites, UN-attenuated. Brightness is applied by the caller.
fn composite_line(row: &LineTableRow, mem: &Memory, y: usize, line: &mut [[u8; 4]]) {
    // 1. backdrop (opaque base).
    let backdrop = unpack_rgb15(mem.cgram[0]);
    for px in line.iter_mut() {
        *px = backdrop;
    }

    // 2. BG.
    if row.mode == 7 {
        let bg = &row.bg[0];
        if bg.visible {
            if let Some(src) = bg.source.as_deref().and_then(|id| mem.sources.get(id)) {
                let mut tmp = vec![0u8; WIDTH * 4];
                render_mode7_scanline(row, src, y, &mut tmp);
                for (x, slot) in line.iter_mut().enumerate() {
                    let p = &tmp[x * 4..x * 4 + 4];
                    if p[3] != 0 {
                        *slot = [p[0], p[1], p[2], 255];
                    }
                }
            }
        }
    } else {
        // Mode 1 (and any non-7 mode in v1): tile layers BG4..BG1.
        for layer in row.bg.iter().rev() {
            for (slot, px) in line.iter_mut().zip(render_bg_layer_scanline(layer, mem, y, WIDTH)) {
                if let Some(c) = px {
                    *slot = c;
                }
            }
        }
    }

    // 3. sprites on top (v1: above all BG).
    for (slot, sp) in line.iter_mut().zip(render_sprite_scanline(mem, y, WIDTH)) {
        if let Some(s) = sp {
            *slot = s.rgba;
        }
    }
}

/// Phase-2 compositor entry: resolved `LineTable` (224 rows) + `Memory` -> full
/// 256x224 RGBA framebuffer (`WIDTH*HEIGHT*4` bytes; alpha always 255). This is
/// the seam the wasm shim (E7) calls.
pub fn render_frame(lt: &LineTable, mem: &Memory) -> Vec<u8> {
    let mut fb = vec![0u8; WIDTH * HEIGHT * 4];
    let mut line = vec![[0u8; 4]; WIDTH];
    let rows = lt.rows.len().min(HEIGHT);
    for y in 0..rows {
        let row = &lt.rows[y];
        composite_line(row, mem, y, &mut line);
        let bri = row.brightness;
        for (x, px) in line.iter().enumerate() {
            let o = (y * WIDTH + x) * 4;
            fb[o] = apply_brightness(px[0], bri);
            fb[o + 1] = apply_brightness(px[1], bri);
            fb[o + 2] = apply_brightness(px[2], bri);
            fb[o + 3] = 255;
        }
    }
    fb
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linetable::LineTableBuilder;
    use crate::memory::{rgb15, Source};
    use crate::registers::{Bg, Obj};

    fn solid_source(color: [u8; 4]) -> Source {
        Source { width: 1, height: 1, rgba: color.to_vec() }
    }

    #[test]
    fn frame_is_full_size_and_opaque() {
        let lt = LineTableBuilder::new(LineTableRow::default()).build(HEIGHT);
        let fb = render_frame(&lt, &Memory::new());
        assert_eq!(fb.len(), WIDTH * HEIGHT * 4);
        assert!(fb.chunks(4).all(|px| px[3] == 255));
    }

    #[test]
    fn empty_memory_is_backdrop_everywhere() {
        let mut mem = Memory::new();
        mem.cgram[0] = rgb15(10, 20, 30);
        let lt = LineTableBuilder::new(LineTableRow::default()).build(HEIGHT);
        let fb = render_frame(&lt, &mem);
        let bd = unpack_rgb15(rgb15(10, 20, 30));
        assert_eq!(&fb[0..4], &bd);
        let last = (HEIGHT * WIDTH - 1) * 4;
        assert_eq!(&fb[last..last + 4], &bd);
    }

    #[test]
    fn brightness_applied_once_uniformly() {
        // A solid red BG source at brightness 0 -> black; at 15 -> red.
        let mut mem = Memory::new();
        mem.sources.insert("bg".into(), solid_source([200, 0, 0, 255]));
        let mut def = LineTableRow::default();
        def.bg[0] = Bg { scroll_x: 0.0, scroll_y: 0.0, source: Some("bg".into()), visible: true };
        def.brightness = 0;
        let lt = LineTableBuilder::new(def.clone()).build(HEIGHT);
        let fb = render_frame(&lt, &mem);
        assert_eq!(&fb[0..4], &[0, 0, 0, 255]); // brightness 0 -> black

        def.brightness = 15;
        let lt = LineTableBuilder::new(def).build(HEIGHT);
        let fb = render_frame(&lt, &mem);
        assert_eq!(&fb[0..4], &[200, 0, 0, 255]); // brightness 15 -> identity
    }

    #[test]
    fn per_line_mode_switch_composites_split_screen() {
        // Mode 1 top band (red BG), Mode 7 bottom band (green floor).
        let mut mem = Memory::new();
        mem.sources.insert("hud".into(), solid_source([200, 0, 0, 255]));
        mem.sources.insert("floor".into(), solid_source([0, 200, 0, 255]));
        let mut def = LineTableRow::default();
        def.mode = 1;
        def.brightness = 15;
        def.bg[0] = Bg { scroll_x: 0.0, scroll_y: 0.0, source: Some("hud".into()), visible: true };
        let mut b = LineTableBuilder::new(def);
        b.hdma(112, 223, |_, r| {
            r.mode = 7;
            r.bg[0].source = Some("floor".into());
            r.m7 = crate::registers::Mode7::default();
        });
        let lt = b.build(HEIGHT);
        let fb = render_frame(&lt, &mem);
        // top band -> red (Mode 1); bottom band -> green (Mode 7).
        let top = (10 * WIDTH + 5) * 4;
        let bot = (200 * WIDTH + 5) * 4;
        assert_eq!(&fb[top..top + 4], &[200, 0, 0, 255]);
        assert_eq!(&fb[bot..bot + 4], &[0, 200, 0, 255]);
    }

    #[test]
    fn sprites_composite_over_bg() {
        let mut mem = Memory::new();
        mem.sources.insert("bg".into(), solid_source([0, 0, 200, 255]));
        // 8x8 sheet, solid yellow opaque.
        mem.sources.insert(
            "sheet".into(),
            Source { width: 8, height: 8, rgba: {
                let mut v = vec![0u8; 8 * 8 * 4];
                for px in v.chunks_mut(4) { px.copy_from_slice(&[200, 200, 0, 255]); }
                v
            } },
        );
        mem.obj_sheet = Some("sheet".into());
        mem.oam[0] = Obj { on: true, x: 0.0, y: 0.0, tile: 0, size: 0, ..Obj::default() };
        let mut def = LineTableRow::default();
        def.brightness = 15;
        def.bg[0] = Bg { scroll_x: 0.0, scroll_y: 0.0, source: Some("bg".into()), visible: true };
        let lt = LineTableBuilder::new(def).build(HEIGHT);
        let fb = render_frame(&lt, &mem);
        // (0,0) covered by sprite -> yellow; far pixel -> blue BG.
        assert_eq!(&fb[0..4], &[200, 200, 0, 255]);
        let far = (100 * WIDTH + 100) * 4;
        assert_eq!(&fb[far..far + 4], &[0, 0, 200, 255]);
    }
}
