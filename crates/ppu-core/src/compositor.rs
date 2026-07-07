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
//! double-applied. (BG and sprite pixel sampling are stubbed during the M4
//! substrate rewrite.)

use crate::bg::{apply_brightness, render_bg_layer_scanline};
use crate::linetable::LineTable;
use crate::memory::{unpack_rgb15, Memory};
use crate::mode7::render_mode7_scanline;
use crate::modes::mode_info;
use crate::registers::RegRow;
use crate::sprite::render_scanline as render_sprite_scanline;
use crate::{HEIGHT, WIDTH};

/// One rung of the per-pixel priority ladder: a BG layer at a given tilemap
/// priority bit, or the OBJ layer at a given sprite-priority level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Slot {
    Bg { layer: usize, prio: bool },
    Obj { prio: u8 },
}

/// Authentic Mode-1 priority ladder, front (index 0) to back. BG participants
/// come from the mode table's `priority_order` ([BG1,BG2,BG3]); the OBJ-prio
/// slots and the BG3-priority-bit lift are Mode-1 hardware semantics. `bg3_high`
/// = BGMODE.3 set: BG3 tile-priority-1 pixels jump above every other layer.
fn mode1_ladder(bg3_high: bool) -> Vec<Slot> {
    let ord = mode_info(1).map_or(&[0u8, 1, 2][..], |m| m.priority_order);
    let (bg1, bg2, bg3) = (ord[0] as usize, ord[1] as usize, ord[2] as usize);
    let mut l = Vec::with_capacity(10);
    if bg3_high {
        l.push(Slot::Bg { layer: bg3, prio: true });
    }
    l.push(Slot::Obj { prio: 3 });
    l.push(Slot::Bg { layer: bg1, prio: true });
    l.push(Slot::Bg { layer: bg2, prio: true });
    l.push(Slot::Obj { prio: 2 });
    l.push(Slot::Bg { layer: bg1, prio: false });
    l.push(Slot::Bg { layer: bg2, prio: false });
    l.push(Slot::Obj { prio: 1 });
    if !bg3_high {
        l.push(Slot::Bg { layer: bg3, prio: true });
    }
    l.push(Slot::Obj { prio: 0 });
    l.push(Slot::Bg { layer: bg3, prio: false });
    l
}

/// Composite one scanline `y` of `row` into `line` (length `WIDTH`), backdrop +
/// BG + sprites, UN-attenuated. Brightness is applied by the caller.
fn composite_line(row: &RegRow, mem: &Memory, y: usize, line: &mut [[u8; 4]]) {
    // 1. backdrop (opaque base).
    let backdrop = unpack_rgb15(mem.cgram[0]);
    for px in line.iter_mut() {
        *px = backdrop;
    }

    // 2. BG.
    if row.mode == 7 {
        if row.bg[0].visible {
            let mut tmp = vec![0u8; WIDTH * 4];
            render_mode7_scanline(row, mem, y, &mut tmp);
            for (x, slot) in line.iter_mut().enumerate() {
                let p = &tmp[x * 4..x * 4 + 4];
                if p[3] != 0 {
                    *slot = [p[0], p[1], p[2], 255];
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
    use crate::memory::rgb15;
    use crate::registers::LineTableRow;

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

    // TODO(m4/bg-raster, m4/mode7, m4/compositing): the BG/Mode-7/sprite compositing tests were
    // deleted with the v1 direct-RGBA `Source` model; they return as VRAM-backed
    // goldens once the rasterizers land.

    #[test]
    fn mode1_ladder_orders_front_to_back() {
        let l = mode1_ladder(false);
        assert_eq!(l.first(), Some(&Slot::Obj { prio: 3 }));
        assert_eq!(l.last(), Some(&Slot::Bg { layer: 2, prio: false }));
        assert!(!l.contains(&Slot::Bg { layer: 3, prio: true })); // BG4 absent in Mode 1
        // bit set: BG3 tile-prio1 lifted to the very front.
        assert_eq!(mode1_ladder(true).first(), Some(&Slot::Bg { layer: 2, prio: true }));
    }

    #[test]
    fn brightness_applied_once_to_backdrop() {
        let mut mem = Memory::new();
        mem.cgram[0] = rgb15(200, 200, 200);
        let mut def = LineTableRow::default();
        def.brightness = 0;
        let lt = LineTableBuilder::new(def.clone()).build(HEIGHT);
        let fb = render_frame(&lt, &mem);
        assert_eq!(&fb[0..4], &[0, 0, 0, 255]); // brightness 0 -> black

        def.brightness = 15;
        let lt = LineTableBuilder::new(def).build(HEIGHT);
        let fb = render_frame(&lt, &mem);
        assert_eq!(&fb[0..4], &unpack_rgb15(rgb15(200, 200, 200)));
    }
}
