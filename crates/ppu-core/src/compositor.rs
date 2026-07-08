//! Phase-2 frame compositor. Ties the per-scanline rasterizers together into the
//! full 256x224 RGBA framebuffer from a resolved `LineTable` + `Memory`.
//!
//! Per scanline `y`:
//!   1. select the active mode from that row (per-line `mode` -> split-screen);
//!   2. start from the backdrop `unpack_rgb15(cgram[0])` (opaque);
//!   3. resolve BG + OBJ:
//!      - Mode 7: the single Mode-7 BG floor (mode7.rs), then sprites overlaid on
//!        top (sprites ordered among themselves by `prio` in `render_scanline`);
//!      - Mode 1: authentic per-pixel priority via [`mode1_ladder`] — each BG
//!        layer's tilemap priority bit interleaved with the mode's layer order
//!        and OBJ priority (0-3), honoring the BG3-priority bit (BGMODE.3);
//!   4. apply INIDISP brightness ONCE to the final pixel (`apply_brightness`).
//!
//! Brightness single-application point: HERE. The scanline primitives this
//! compositor calls all return un-attenuated direct RGBA, so brightness is never
//! double-applied.

use crate::bg::{apply_brightness, render_bg_layer_scanline_px, BgPixel};
use crate::linetable::LineTable;
use crate::memory::{unpack_rgb15, Memory};
use crate::mode7::render_mode7_scanline;
use crate::modes::mode_info;
use crate::registers::RegRow;
use crate::sprite::render_scanline as render_sprite_scanline;
use crate::window::in_window;
use crate::{HEIGHT, WIDTH};

/// One rung of the per-pixel priority ladder: a BG layer at a given tilemap
/// priority bit, or the OBJ layer at a given sprite-priority level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Slot {
    Bg { layer: usize, prio: bool },
    Obj { prio: u8 },
}

/// Is the layer behind ladder rung `slot` enabled on the screen whose
/// designation bitmask is `mask` (TM for main, TS for sub)? Bits 0-4 =
/// BG1,BG2,BG3,BG4,OBJ.
fn slot_enabled(mask: u8, slot: &Slot) -> bool {
    let bit = match slot {
        Slot::Bg { layer, .. } => *layer as u8,
        Slot::Obj { .. } => 4,
    };
    mask & (1 << bit) != 0
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
        l.push(Slot::Bg {
            layer: bg3,
            prio: true,
        });
    }
    l.push(Slot::Obj { prio: 3 });
    l.push(Slot::Bg {
        layer: bg1,
        prio: true,
    });
    l.push(Slot::Bg {
        layer: bg2,
        prio: true,
    });
    l.push(Slot::Obj { prio: 2 });
    l.push(Slot::Bg {
        layer: bg1,
        prio: false,
    });
    l.push(Slot::Bg {
        layer: bg2,
        prio: false,
    });
    l.push(Slot::Obj { prio: 1 });
    if !bg3_high {
        l.push(Slot::Bg {
            layer: bg3,
            prio: true,
        });
    }
    l.push(Slot::Obj { prio: 0 });
    l.push(Slot::Bg {
        layer: bg3,
        prio: false,
    });
    l
}

fn tile_mode_ladder(mode: u8) -> Vec<Slot> {
    let Some(info) = mode_info(mode) else {
        return vec![
            Slot::Obj { prio: 3 },
            Slot::Obj { prio: 2 },
            Slot::Obj { prio: 1 },
            Slot::Obj { prio: 0 },
        ];
    };
    let mut l = Vec::with_capacity(info.priority_order.len() * 2 + 4);
    l.push(Slot::Obj { prio: 3 });
    for &layer in info.priority_order {
        l.push(Slot::Bg {
            layer: layer as usize,
            prio: true,
        });
    }
    l.push(Slot::Obj { prio: 2 });
    for &layer in info.priority_order {
        l.push(Slot::Bg {
            layer: layer as usize,
            prio: false,
        });
    }
    l.push(Slot::Obj { prio: 1 });
    l.push(Slot::Obj { prio: 0 });
    l
}

/// Composite one scanline `y` of `row` into `line` for ONE screen, including only
/// the layers whose bit is set in `mask` (TM = main, TS = sub). `wmask` is the
/// window-designation register for that screen (TMW = main, TSW = sub): a layer
/// whose `wmask` bit is set is suppressed at any x inside that layer's combined
/// window. `pub(crate)` so the color-math ticket resolves the sub line via
/// `composite_screen(.., row.ts, row.tsw, ..)`.
pub(crate) fn composite_screen(
    row: &RegRow,
    mem: &Memory,
    y: usize,
    mask: u8,
    wmask: u8,
    line: &mut [[u8; 4]],
) {
    // 1. backdrop (opaque base).
    let backdrop = unpack_rgb15(mem.cgram[0]);
    for px in line.iter_mut() {
        *px = backdrop;
    }

    // Window suppression: for each layer (0..3 = BG1..BG4, 4 = OBJ) whose wmask
    // bit is set, precompute which columns fall inside its combined window (and
    // are therefore hidden on this screen). `None` = layer not windowed here.
    let ranges = row.window_ranges();
    let win_hidden: [Option<Vec<bool>>; 5] = std::array::from_fn(|layer| {
        if wmask & (1 << layer) != 0 {
            let sel = row.layer_window(layer);
            Some((0..WIDTH).map(|x| in_window(&sel, &ranges, x)).collect())
        } else {
            None
        }
    });
    let hidden = |layer: usize, x: usize| -> bool {
        win_hidden[layer].as_ref().is_some_and(|m| m[x])
    };

    // 2. BG + OBJ.
    if row.mode == 7 {
        // Mode 7 is a single BG layer (mode7.rs owns its own compositing);
        // sprites still overlay on top, ordered among themselves by prio.
        if row.bg[0].visible && mask & 0x01 != 0 {
            let mut tmp = vec![0u8; WIDTH * 4];
            render_mode7_scanline(row, mem, y, &mut tmp);
            for (x, slot) in line.iter_mut().enumerate() {
                if hidden(0, x) {
                    continue;
                }
                let p = &tmp[x * 4..x * 4 + 4];
                if p[3] != 0 {
                    *slot = [p[0], p[1], p[2], 255];
                }
            }
        }
        if mask & (1 << 4) != 0 {
            for (x, (slot, sp)) in line
                .iter_mut()
                .zip(render_sprite_scanline(mem, y, WIDTH))
                .enumerate()
            {
                if hidden(4, x) {
                    continue;
                }
                if let Some(s) = sp {
                    *slot = s.rgba;
                }
            }
        }
    } else {
        // Tile modes: per-pixel priority resolution. Each BG layer and the OBJ
        // layer produce one candidate per x; the ladder (front->back) picks the
        // frontmost occupied rung, interleaving tilemap priority bit x mode layer
        // order x sprite priority. Backdrop shows through if no rung hits.
        let bgs: Vec<Vec<Option<BgPixel>>> = row
            .bg
            .iter()
            .map(|l| render_bg_layer_scanline_px(l, mem, y, WIDTH))
            .collect();
        let obj = render_sprite_scanline(mem, y, WIDTH);
        let ladder = if row.mode == 1 {
            mode1_ladder(row.bg3_priority)
        } else {
            tile_mode_ladder(row.mode)
        };
        for (x, slot) in line.iter_mut().enumerate() {
            for rung in &ladder {
                if !slot_enabled(mask, rung) {
                    continue;
                }
                let layer_bit = match rung {
                    Slot::Bg { layer, .. } => *layer,
                    Slot::Obj { .. } => 4,
                };
                if hidden(layer_bit, x) {
                    continue;
                }
                let hit = match *rung {
                    Slot::Bg { layer, prio } => {
                        bgs[layer][x].filter(|p| p.prio == prio).map(|p| p.rgba)
                    }
                    Slot::Obj { prio } => obj[x].filter(|s| s.prio == prio).map(|s| s.rgba),
                };
                if let Some(rgba) = hit {
                    *slot = rgba;
                    break;
                }
            }
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
        composite_screen(row, mem, y, row.tm, row.tmw, &mut line);
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
    use crate::registers::{LineTableRow, Obj};

    /// Set pixel (0,0) of 4bpp char `c` (at VRAM word `char_base`) to palette
    /// index 1 (plane-0 bit 7 of the char's row 0). A char is 16 words at 4bpp.
    fn put_px(m: &mut Memory, char_base: usize, c: usize) {
        m.vram[char_base + c * 16] = 0x0080;
    }

    #[test]
    fn tile_priority_bit_lifts_bg2_over_bg1() {
        // BG1 (pal 1 = red) and BG2 (pal 0 = green) both draw index 1 at (0,0).
        // At equal tile priority BG1 wins (front of BG2); setting BG2's tilemap
        // priority bit lifts BG2 above BG1.
        let mut m = Memory::new();
        m.cgram[0] = rgb15(0, 0, 0);
        m.cgram[16 + 1] = rgb15(255, 0, 0); // BG1 sub-palette 1, index 1
        m.cgram[1] = rgb15(0, 255, 0); // BG2 sub-palette 0, index 1
        put_px(&mut m, 0x1000, 1); // shared char 1
        m.vram[0x0000] = 1 | (1 << 10); // BG1 map(0,0): tile 1, pal 1, prio 0
        let mut src = LineTableRow::default();
        src.bg[0].char_base = 0x1000;
        src.bg[1].char_base = 0x1000;
        src.bg[1].map_base = 0x0400;
        // BG2 priority 0 -> BG1 (red) wins at equal priority.
        m.vram[0x0400] = 1; // BG2 map(0,0): tile 1, pal 0, prio 0
        let lt = LineTableBuilder::new(src.clone()).build(HEIGHT);
        assert_eq!(
            &render_frame(&lt, &m)[0..4],
            &unpack_rgb15(rgb15(255, 0, 0))
        );
        // BG2 priority 1 -> BG2 (green) lifts above BG1.
        m.vram[0x0400] = 1 | (1 << 13);
        let lt = LineTableBuilder::new(src).build(HEIGHT);
        assert_eq!(
            &render_frame(&lt, &m)[0..4],
            &unpack_rgb15(rgb15(0, 255, 0))
        );
    }

    #[test]
    fn bg3_priority_bit_lifts_bg3_over_bg1() {
        // BG1 (red) draws index 1 at (0,0); BG3 (blue, tilemap-priority set) also
        // draws index 1. Normally BG3's priority-1 rung sits below BG1; the
        // BGMODE.3 BG3-priority bit lifts BG3 priority-1 above every layer.
        let mut m = Memory::new();
        m.cgram[0] = rgb15(0, 0, 0);
        m.cgram[1] = rgb15(255, 0, 0); // BG1 sub-palette 0, index 1
        m.cgram[4 + 1] = rgb15(0, 0, 255); // BG3 2bpp sub-palette 1 (base 4), index 1
        put_px(&mut m, 0x1000, 1); // BG1 4bpp char 1
        m.vram[0x2000 + 8] = 0x0080; // BG3 2bpp char 1 (8 words/char), pixel (0,0) = 1
        m.vram[0x0000] = 1; // BG1 map(0,0): tile 1, pal 0, prio 0
        m.vram[0x0800] = 1 | (1 << 10) | (1 << 13); // BG3 map: tile 1, pal 1, prio 1
        let mut src = LineTableRow::default();
        src.bg[0].char_base = 0x1000;
        src.bg[2].char_base = 0x2000;
        src.bg[2].map_base = 0x0800;
        // Bit clear: BG1 (red) wins over BG3's low-slung priority-1 rung.
        let lt = LineTableBuilder::new(src.clone()).build(HEIGHT);
        assert_eq!(
            &render_frame(&lt, &m)[0..4],
            &unpack_rgb15(rgb15(255, 0, 0))
        );
        // Bit set: BG3 priority-1 (blue) jumps to the very front.
        src.bg3_priority = true;
        let lt = LineTableBuilder::new(src).build(HEIGHT);
        assert_eq!(
            &render_frame(&lt, &m)[0..4],
            &unpack_rgb15(rgb15(0, 0, 255))
        );
    }

    #[test]
    fn sprite_priority_interleaves_with_bg() {
        // BG1 draws index 1 (red) with its tilemap priority bit set. A sprite
        // (yellow) at the same pixel sits above BG1 when OBJ prio 3, below when
        // OBJ prio 0 — the ladder interleaves OBJ priority with the BG rungs.
        let mut m = Memory::new();
        m.cgram[0] = rgb15(0, 0, 0);
        m.cgram[1] = rgb15(255, 0, 0); // BG1 pal 0, index 1
        m.cgram[128 + 1] = rgb15(255, 255, 0); // OBJ pal 0, index 1
        put_px(&mut m, 0x1000, 1);
        m.vram[0x0000] = 1 | (1 << 13); // BG1 map(0,0): tile 1, priority 1
        m.obsel.char_base = 0x4000;
        m.vram[0x4000 + 16] = 0x0080; // OBJ char 1 (16 words), pixel (0,0) = 1
        let mut src = LineTableRow::default();
        src.bg[0].char_base = 0x1000;
        m.oam[0] = Obj {
            on: true,
            x: 0,
            y: 0,
            tile: 1,
            prio: 3,
            ..Obj::default()
        };
        // OBJ prio 3 is above BG1 priority-1: sprite (yellow) wins.
        let lt = LineTableBuilder::new(src.clone()).build(HEIGHT);
        assert_eq!(
            &render_frame(&lt, &m)[0..4],
            &unpack_rgb15(rgb15(255, 255, 0))
        );
        // OBJ prio 0 is below BG1 priority-1: BG1 (red) wins.
        m.oam[0].prio = 0;
        let lt = LineTableBuilder::new(src).build(HEIGHT);
        assert_eq!(
            &render_frame(&lt, &m)[0..4],
            &unpack_rgb15(rgb15(255, 0, 0))
        );
    }

    #[test]
    fn tile_modes_2_and_3_dispatch_drawable_layers() {
        for (mode, layer) in [(2u8, 0usize), (3, 1)] {
            let mut mem = Memory::new();
            mem.cgram[0] = rgb15(0, 0, 0);
            mem.cgram[1] = rgb15(0, 255, 0);
            put_px(&mut mem, 0x1000, 1);
            mem.vram[0] = 1;

            let mut src = LineTableRow::default();
            src.mode = mode;
            src.bg[layer].char_base = 0x1000;
            let lt = LineTableBuilder::new(src).build(HEIGHT);

            assert_eq!(
                &render_frame(&lt, &mem)[0..4],
                &unpack_rgb15(rgb15(0, 255, 0)),
                "mode {mode} layer {layer} should draw"
            );
        }
    }

    #[test]
    fn mode4_dispatches_2bpp_bg2() {
        let mut mem = Memory::new();
        mem.cgram[0] = rgb15(0, 0, 0);
        mem.cgram[1] = rgb15(0, 255, 0);
        mem.vram[0x1000 + 8] = 0x0080;
        mem.vram[0] = 1;

        let mut src = LineTableRow::default();
        src.mode = 4;
        src.bg[1].char_base = 0x1000;
        let lt = LineTableBuilder::new(src).build(HEIGHT);

        assert_eq!(
            &render_frame(&lt, &mem)[0..4],
            &unpack_rgb15(rgb15(0, 255, 0))
        );
    }

    #[test]
    fn mode0_bg4_dispatches_with_fourth_cgram_band() {
        let mut mem = Memory::new();
        mem.cgram[0] = rgb15(0, 0, 0);
        mem.cgram[24 * 4 + 1] = rgb15(0, 0, 255);
        mem.vram[0x3000 + 8] = 0x0080;
        mem.vram[0] = 1;

        let mut src = LineTableRow::default();
        src.mode = 0;
        src.bg[3].char_base = 0x3000;
        let lt = LineTableBuilder::new(src).build(HEIGHT);

        assert_eq!(
            &render_frame(&lt, &mem)[0..4],
            &unpack_rgb15(rgb15(0, 0, 255))
        );
    }

    #[test]
    fn unsupported_tile_modes_still_render_obj() {
        let mut mem = Memory::new();
        mem.cgram[0] = rgb15(0, 0, 0);
        mem.cgram[128 + 1] = rgb15(255, 255, 0);
        mem.obsel.char_base = 0x4000;
        mem.vram[0x4000 + 16] = 0x0080;
        mem.oam[0] = Obj {
            on: true,
            x: 0,
            y: 0,
            tile: 1,
            prio: 3,
            ..Obj::default()
        };

        let mut src = LineTableRow::default();
        src.mode = 5;
        let lt = LineTableBuilder::new(src).build(HEIGHT);

        assert_eq!(
            &render_frame(&lt, &mem)[0..4],
            &unpack_rgb15(rgb15(255, 255, 0))
        );
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

    // TODO(m4/bg-raster, m4/mode7, m4/compositing): the BG/Mode-7/sprite compositing tests were
    // deleted with the v1 direct-RGBA `Source` model; they return as VRAM-backed
    // goldens once the rasterizers land.

    #[test]
    fn mode1_ladder_orders_front_to_back() {
        let l = mode1_ladder(false);
        assert_eq!(l.first(), Some(&Slot::Obj { prio: 3 }));
        assert_eq!(
            l.last(),
            Some(&Slot::Bg {
                layer: 2,
                prio: false
            })
        );
        assert!(!l.contains(&Slot::Bg {
            layer: 3,
            prio: true
        })); // BG4 absent in Mode 1
             // bit set: BG3 tile-prio1 lifted to the very front.
        assert_eq!(
            mode1_ladder(true).first(),
            Some(&Slot::Bg {
                layer: 2,
                prio: true
            })
        );
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

    #[test]
    fn tm_masks_layer_from_main_but_ts_keeps_it_on_sub() {
        // BG1 draws index 1 (red) at (0,0) over a black backdrop.
        let mut m = Memory::new();
        m.cgram[0] = rgb15(0, 0, 0);
        m.cgram[1] = rgb15(255, 0, 0); // BG1 pal 0, index 1
        put_px(&mut m, 0x1000, 1);
        m.vram[0x0000] = 1; // BG1 map(0,0): tile 1
        let mut src = LineTableRow::default();
        src.bg[0].char_base = 0x1000;
        src.tm = 0x1e; // main: BG1 (bit 0) masked off, others on
        src.ts = 0x01; // sub: BG1 enabled
        let row = RegRow::from(&src);
        let mut main = vec![[0u8; 4]; WIDTH];
        let mut sub = vec![[0u8; 4]; WIDTH];
        composite_screen(&row, &m, 0, row.tm, row.tmw, &mut main);
        composite_screen(&row, &m, 0, row.ts, row.tsw, &mut sub);
        // main: BG1 masked -> backdrop (black) shows through.
        assert_eq!(main[0], unpack_rgb15(rgb15(0, 0, 0)));
        // sub: BG1 enabled -> red.
        assert_eq!(sub[0], unpack_rgb15(rgb15(255, 0, 0)));
    }

    #[test]
    fn tmw_clips_layer_to_a_horizontal_band_on_main() {
        // BG1 draws index 1 (red) across the row over a black backdrop.
        let mut m = Memory::new();
        m.cgram[0] = rgb15(0, 0, 0);
        m.cgram[1] = rgb15(255, 0, 0);
        put_px(&mut m, 0x1000, 1);
        for tx in 0..32 {
            m.vram[tx] = 1; // BG1 map row 0: tile 1 across the width
        }
        let mut src = LineTableRow::default();
        src.bg[0].char_base = 0x1000;
        // Window 1 = [0,7] (first tile-column band). BG1 W1 enable.
        src.wh0 = 0;
        src.wh1 = 7;
        src.w12sel = 0x02; // BG1 low nibble: W1 enable
        src.wbglog = 0x00; // BG1 logic OR (single window)
        src.tmw = 0x01; // suppress BG1 inside window on the MAIN screen
        let row = RegRow::from(&src);
        let mut main = vec![[0u8; 4]; WIDTH];
        composite_screen(&row, &m, 0, row.tm, row.tmw, &mut main);
        // Inside the window (x=0..7): BG1 suppressed -> backdrop (black).
        assert_eq!(main[0], unpack_rgb15(rgb15(0, 0, 0)));
        assert_eq!(main[7], unpack_rgb15(rgb15(0, 0, 0)));
        // Outside the window (x>=8): BG1 shows (red).
        assert_eq!(main[8], unpack_rgb15(rgb15(255, 0, 0)));
        assert_eq!(main[128], unpack_rgb15(rgb15(255, 0, 0)));
    }

    #[test]
    fn tsw_clips_layer_to_a_band_on_sub_only() {
        // Same BG1-across-the-row setup; TSW clips the SUB screen, TMW leaves main.
        let mut m = Memory::new();
        m.cgram[0] = rgb15(0, 0, 0);
        m.cgram[1] = rgb15(255, 0, 0);
        put_px(&mut m, 0x1000, 1);
        for tx in 0..32 {
            m.vram[tx] = 1;
        }
        let mut src = LineTableRow::default();
        src.bg[0].char_base = 0x1000;
        src.ts = 0x01; // BG1 enabled on the sub screen
        src.wh0 = 0;
        src.wh1 = 7;
        src.w12sel = 0x02; // BG1 W1 enable
        src.tsw = 0x01; // suppress BG1 inside window on the SUB screen
        // tmw stays 0 -> main screen unaffected.
        let row = RegRow::from(&src);
        let mut main = vec![[0u8; 4]; WIDTH];
        let mut sub = vec![[0u8; 4]; WIDTH];
        composite_screen(&row, &m, 0, row.tm, row.tmw, &mut main);
        composite_screen(&row, &m, 0, row.ts, row.tsw, &mut sub);
        // Main screen: no TMW -> BG1 red everywhere.
        assert_eq!(main[0], unpack_rgb15(rgb15(255, 0, 0)));
        // Sub screen: BG1 clipped inside window -> backdrop; visible outside.
        assert_eq!(sub[0], unpack_rgb15(rgb15(0, 0, 0)));
        assert_eq!(sub[8], unpack_rgb15(rgb15(255, 0, 0)));
    }
}
