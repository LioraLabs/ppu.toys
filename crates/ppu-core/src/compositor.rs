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
use crate::memory::{rgb15, unpack_rgb15, Memory};
use crate::mode7::{render_mode7_scanline, render_mode7_scanline_px};
use crate::modes::mode_info;
use crate::registers::RegRow;
use crate::sprite::{bin_line, render_scanline_for};
use crate::window::in_window;
use crate::ObjOverflow;
use crate::{HEIGHT, WIDTH};

/// Per-channel SNES color math in 15-bit BGR space. Add or subtract each 5-bit
/// channel. Without half: saturate to 0..31 (add caps at 31, subtract floors at
/// 0). With half: the result is halved (`>> 1`) instead of upper-clamped — on
/// hardware `(main + sub) >> 1` needs no cap since it can't exceed 31, and this
/// is what makes a ½-add read as clean 50% translucency. Subtract still floors
/// at 0 before halving.
fn color_math(main: u16, sub: u16, subtract: bool, half: bool) -> u16 {
    let ch = |shift: u32| {
        let m = ((main >> shift) & 0x1f) as i16;
        let s = ((sub >> shift) & 0x1f) as i16;
        let raw = if subtract { m - s } else { m + s };
        let v = if half {
            raw.max(0) >> 1 // floor at 0 (subtract), then halve; add sum never exceeds 62
        } else {
            raw.clamp(0, 31)
        };
        (v as u16) << shift
    };
    ch(0) | ch(5) | ch(10)
}

/// Resolve a CGWSEL 2-bit region field against whether column x is inside the
/// color window: 0 = never, 1 = outside, 2 = inside, 3 = always.
fn region_active(field: u8, inside: bool) -> bool {
    match field & 0x03 {
        0 => false,
        1 => !inside,
        2 => inside,
        _ => true,
    }
}

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

/// Which layer produced a resolved pixel, for the color-math blend: backdrop,
/// a BG layer (0..3), or a sprite (carrying its OBJ palette for the 4-7 gate).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PixelSource {
    Backdrop,
    Bg(u8),
    Obj { pal: u8 },
}

impl PixelSource {
    /// CGADSUB math-enable bit index: BG1..BG4 = 0..3, OBJ = 4, backdrop = 5.
    fn math_layer(self) -> usize {
        match self {
            PixelSource::Backdrop => 5,
            PixelSource::Bg(l) => l as usize,
            PixelSource::Obj { .. } => 4,
        }
    }
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

/// Authentic Mode 7 EXTBG priority ladder, front (index 0) to back. SETINI.6
/// splits the single Mode-7 plane into two priority sub-levels by each pixel's
/// bit 7: HIGH pixels (`prio: true`) sit above OBJ prio 1-2, LOW pixels
/// (`prio: false`) below them — so a sprite at OBJ prio 1/2 renders BETWEEN the
/// two floor levels. `layer: 0` is the Mode-7 plane (TM/TS BG1 bit 0). Pinned by
/// `mode7_extbg_ladder_orders_front_to_back`; do not reorder without a hardware
/// reference.
fn mode7_extbg_ladder() -> [Slot; 6] {
    [
        Slot::Obj { prio: 3 },
        Slot::Bg {
            layer: 0,
            prio: true,
        }, // Mode-7 high (bit7 = 1)
        Slot::Obj { prio: 2 },
        Slot::Obj { prio: 1 },
        Slot::Bg {
            layer: 0,
            prio: false,
        }, // Mode-7 low (bit7 = 0)
        Slot::Obj { prio: 0 },
    ]
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
#[allow(clippy::too_many_arguments)]
pub(crate) fn composite_screen(
    row: &RegRow,
    mem: &Memory,
    y: usize,
    mask: u8,
    wmask: u8,
    obj: &[usize],
    line: &mut [[u8; 4]],
    src: &mut [PixelSource],
) {
    // 1. backdrop (opaque base).
    let backdrop = unpack_rgb15(mem.cgram[0]);
    for px in line.iter_mut() {
        *px = backdrop;
    }
    for s in src.iter_mut() {
        *s = PixelSource::Backdrop;
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
    let hidden =
        |layer: usize, x: usize| -> bool { win_hidden[layer].as_ref().is_some_and(|m| m[x]) };

    // 2. BG + OBJ.
    if row.mode == 7 {
        if row.extbg() {
            // EXTBG: the Mode-7 plane splits into two per-pixel priority levels
            // (bit 7) interleaved with OBJ prio via `mode7_extbg_ladder`. Resolve
            // per pixel like the tile-mode path; the only BG participant is the
            // Mode-7 plane at layer 0.
            let m7 = if row.bg[0].visible {
                render_mode7_scanline_px(row, mem, y, WIDTH)
            } else {
                vec![None; WIDTH]
            };
            let obj = render_scanline_for(mem, obj, y, WIDTH);
            let ladder = mode7_extbg_ladder();
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
                        Slot::Bg { prio, .. } => m7[x]
                            .filter(|p| p.prio == prio)
                            .map(|p| (p.rgba, PixelSource::Bg(0))),
                        Slot::Obj { prio } => obj[x]
                            .filter(|s| s.prio == prio)
                            .map(|s| (s.rgba, PixelSource::Obj { pal: s.pal })),
                    };
                    if let Some((rgba, source)) = hit {
                        *slot = rgba;
                        src[x] = source;
                        break;
                    }
                }
            }
        } else {
            // EXTBG off (today's behavior, byte-identical): single Mode-7 floor,
            // sprites overlaid on top ordered among themselves by prio.
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
                        src[x] = PixelSource::Bg(0);
                    }
                }
            }
            if mask & (1 << 4) != 0 {
                for (x, (slot, sp)) in line
                    .iter_mut()
                    .zip(render_scanline_for(mem, obj, y, WIDTH))
                    .enumerate()
                {
                    if hidden(4, x) {
                        continue;
                    }
                    if let Some(s) = sp {
                        *slot = s.rgba;
                        src[x] = PixelSource::Obj { pal: s.pal };
                    }
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
        let obj = render_scanline_for(mem, obj, y, WIDTH);
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
                    Slot::Bg { layer, prio } => bgs[layer][x]
                        .filter(|p| p.prio == prio)
                        .map(|p| (p.rgba, PixelSource::Bg(layer as u8))),
                    Slot::Obj { prio } => obj[x]
                        .filter(|s| s.prio == prio)
                        .map(|s| (s.rgba, PixelSource::Obj { pal: s.pal })),
                };
                if let Some((rgba, source)) = hit {
                    *slot = rgba;
                    src[x] = source;
                    break;
                }
            }
        }
    }
}

/// Blend one resolved MAIN pixel against the SUB screen (or COLDATA), returning
/// the pre-brightness RGBA plus whether color math was applied. Applies
/// CGADSUB add/sub/half gated by the main source layer's enable bit and (for
/// sprites) the OBJ palette-4-7 rule, plus the CGWSEL color-window regions:
/// `clip` forces the main pixel to black before math and suppresses half
/// (even when math is disabled for the layer), and `prevent` disables math
/// for the column entirely. `main`/`sub` are the resolved RGBA pixels;
/// `src_main`/`src_sub` their sources.
fn blend_pixel(
    row: &RegRow,
    main: [u8; 4],
    sub: [u8; 4],
    src_main: PixelSource,
    src_sub: PixelSource,
    clip: bool,
    prevent: bool,
) -> ([u8; 4], bool) {
    let math_enabled = !prevent
        && row.math_layer_enabled(src_main.math_layer())
        && match src_main {
            PixelSource::Obj { pal } => pal >= 4,
            _ => true,
        };
    if !math_enabled {
        // Clip still blackens the main pixel even when math is off for it.
        return (if clip { [0, 0, 0, 255] } else { main }, false);
    }
    let main15 = if clip {
        0
    } else {
        rgb15(main[0], main[1], main[2])
    };
    let (sub15, sub_is_backdrop) = if row.add_subscreen() {
        (
            rgb15(sub[0], sub[1], sub[2]),
            src_sub == PixelSource::Backdrop,
        )
    } else {
        (row.coldata, false)
    };
    // Half is suppressed inside clip-to-black and when the sub pixel is backdrop.
    let half = row.math_half() && !clip && !sub_is_backdrop;
    (
        unpack_rgb15(color_math(main15, sub15, row.math_subtract(), half)),
        true,
    )
}

/// Phase-2 compositor entry: resolved `LineTable` (224 rows) + `Memory` -> full
/// 256x224 RGBA framebuffer. Framebuffer-only convenience over
/// [`render_frame_stats`] (drops the STAT77 diagnostic).
pub fn render_frame(lt: &LineTable, mem: &Memory) -> Vec<u8> {
    render_frame_stats(lt, mem).0
}

/// Everything the compositor computed for one frame: the final framebuffer
/// plus the per-screen intermediates the inspector renders (M9 view seam).
/// View data only — `framebuffer` is byte-identical to `render_frame`'s
/// output; the compositor's rendering semantics are unchanged.
pub struct FrameView {
    /// 256x224 RGBA, post-math post-brightness (THE framebuffer).
    pub framebuffer: Vec<u8>,
    pub overflow: ObjOverflow,
    /// Main-screen composite, pre-color-math, pre-brightness (RGBA).
    pub main: Vec<u8>,
    /// Sub-screen composite, pre-color-math, pre-brightness (RGBA).
    pub sub: Vec<u8>,
    /// One byte per pixel: bit0 = color math applied, bit1 = clip-to-black
    /// region active, bit2 = prevent-math region active. Force-blank lines = 0.
    pub math_mask: Vec<u8>,
}

/// Compositor entry that ALSO returns the per-frame OBJ overflow diagnostic
/// (`$213E` STAT77) and the pre-math/pre-brightness intermediates (M9 view
/// seam). The per-line OBJ bin is computed ONCE here and reused by the main
/// and sub `composite_screen` passes, so both screens composite the exact
/// same deterministically-capped sprite set (no drift).
pub fn render_frame_view(lt: &LineTable, mem: &Memory) -> FrameView {
    let mut fb = vec![0u8; WIDTH * HEIGHT * 4];
    let mut vmain = vec![0u8; WIDTH * HEIGHT * 4];
    let mut vsub = vec![0u8; WIDTH * HEIGHT * 4];
    let mut vmask = vec![0u8; WIDTH * HEIGHT];
    let mut main = vec![[0u8; 4]; WIDTH];
    let mut sub = vec![[0u8; 4]; WIDTH];
    let mut src_main = vec![PixelSource::Backdrop; WIDTH];
    let mut src_sub = vec![PixelSource::Backdrop; WIDTH];
    let mut stats = ObjOverflow::default();
    let rows = lt.rows.len().min(HEIGHT);
    for y in 0..rows {
        let row = &lt.rows[y];
        if row.force_blank {
            // Forced blank: no rendering this line; output opaque black.
            for x in 0..WIDTH {
                let o = (y * WIDTH + x) * 4;
                fb[o..o + 4].copy_from_slice(&[0, 0, 0, 255]);
                vmain[o..o + 4].copy_from_slice(&[0, 0, 0, 255]);
                vsub[o..o + 4].copy_from_slice(&[0, 0, 0, 255]);
            }
            continue;
        }
        // Bin OBJ once per line; both screens reuse the identical capped set.
        let bin = bin_line(mem, y);
        stats.range_over |= bin.range_over;
        stats.time_over |= bin.time_over;
        stats.max_sprites = stats.max_sprites.max(bin.sprite_count);
        stats.max_tiles = stats.max_tiles.max(bin.tile_count);
        composite_screen(
            row,
            mem,
            y,
            row.tm,
            row.tmw,
            &bin.sprites,
            &mut main,
            &mut src_main,
        );
        composite_screen(
            row,
            mem,
            y,
            row.ts,
            row.tsw,
            &bin.sprites,
            &mut sub,
            &mut src_sub,
        );
        let row_off = y * WIDTH * 4;
        for x in 0..WIDTH {
            vmain[row_off + x * 4..row_off + x * 4 + 4].copy_from_slice(&main[x]);
            vsub[row_off + x * 4..row_off + x * 4 + 4].copy_from_slice(&sub[x]);
        }
        let bri = row.brightness;
        let cw_sel = row.color_window();
        let ranges = row.window_ranges();
        let clip_mode = row.clip_mode();
        let prevent_mode = row.prevent_mode();
        for x in 0..WIDTH {
            let inside = in_window(&cw_sel, &ranges, x);
            let clip = region_active(clip_mode, inside);
            let prevent = region_active(prevent_mode, inside);
            let (px, applied) =
                blend_pixel(row, main[x], sub[x], src_main[x], src_sub[x], clip, prevent);
            vmask[y * WIDTH + x] = applied as u8 | ((clip as u8) << 1) | ((prevent as u8) << 2);
            let o = (y * WIDTH + x) * 4;
            fb[o] = apply_brightness(px[0], bri);
            fb[o + 1] = apply_brightness(px[1], bri);
            fb[o + 2] = apply_brightness(px[2], bri);
            fb[o + 3] = 255;
        }
    }
    FrameView {
        framebuffer: fb,
        overflow: stats,
        main: vmain,
        sub: vsub,
        math_mask: vmask,
    }
}

/// Compositor entry that ALSO returns the per-frame OBJ overflow diagnostic
/// (`$213E` STAT77). The framebuffer/stats half of [`render_frame_view`],
/// which owns the actual render loop.
pub fn render_frame_stats(lt: &LineTable, mem: &Memory) -> (Vec<u8>, ObjOverflow) {
    let v = render_frame_view(lt, mem);
    (v.framebuffer, v.overflow)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linetable::LineTableBuilder;
    use crate::registers::{LineTableRow, Obj};

    /// Pack 5-bit channels straight into a BGR555 word (unlike `rgb15` which
    /// takes 8-bit inputs) — lets the color-math tests assert exact 5-bit values.
    fn rgb15_word(r: u16, g: u16, b: u16) -> u16 {
        (b << 10) | (g << 5) | r
    }

    #[test]
    fn color_math_adds_clamps_and_halves_per_channel() {
        // r: 5+3=8; g: 20+20=40 clamp 31; b: 0+7=7. No half.
        let main = rgb15_word(5, 20, 0);
        let sub = rgb15_word(3, 20, 7);
        assert_eq!(color_math(main, sub, false, false), rgb15_word(8, 31, 7));
        // Half halves the RAW per-channel sums (no pre-clamp): (5+3)>>1=4,
        // (20+20)>>1=20, (0+7)>>1=3 -> (4, 20, 3). This is the clean-50% path.
        assert_eq!(color_math(main, sub, false, true), rgb15_word(4, 20, 3));
    }

    #[test]
    fn color_math_subtracts_and_floors_at_zero() {
        // r: 10-3=7; g: 3-10 -> 0 (saturating); b: 31-1=30.
        let main = rgb15_word(10, 3, 31);
        let sub = rgb15_word(3, 10, 1);
        assert_eq!(color_math(main, sub, true, false), rgb15_word(7, 0, 30));
        // Half of (7,0,30) = (3,0,15).
        assert_eq!(color_math(main, sub, true, true), rgb15_word(3, 0, 15));
    }

    #[test]
    fn region_active_maps_the_two_bit_field() {
        // field: 0=never, 1=outside(!inside), 2=inside, 3=always.
        assert_eq!(
            [region_active(0, false), region_active(0, true)],
            [false, false]
        );
        assert_eq!(
            [region_active(1, false), region_active(1, true)],
            [true, false]
        );
        assert_eq!(
            [region_active(2, false), region_active(2, true)],
            [false, true]
        );
        assert_eq!(
            [region_active(3, false), region_active(3, true)],
            [true, true]
        );
    }

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
    fn mosaic_does_not_pixelate_sprites() {
        // A lit sprite pixel at (0,0) only; BG1 mosaic on. The sprite must NOT
        // replicate into x=1 (sprites are never mosaiced).
        let mut m = Memory::new();
        m.cgram[0] = rgb15(0, 0, 0);
        m.cgram[128 + 1] = rgb15(255, 255, 0); // OBJ pal0 idx1
        m.obsel.char_base = 0x4000;
        m.vram[0x4000 + 16] = 0x0080; // OBJ char 1 pixel (0,0) only
        m.oam[0] = Obj {
            on: true,
            x: 0,
            y: 0,
            tile: 1,
            prio: 3,
            ..Obj::default()
        };
        let mut src = LineTableRow::default();
        src.tm = 0x10; // main: OBJ only
        src.mosaic_size = 3;
        src.mosaic_enable = [true, true, true, true]; // all BG enabled (irrelevant to OBJ)
        let lt = LineTableBuilder::new(src).build(HEIGHT);
        let fb = render_frame(&lt, &m);
        // x=0: sprite yellow. x=1: NOT replicated -> backdrop (black).
        assert_eq!(&fb[0..4], &unpack_rgb15(rgb15(255, 255, 0)));
        assert_eq!(&fb[4..8], &unpack_rgb15(rgb15(0, 0, 0)));
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
    fn mode7_extbg_ladder_orders_front_to_back() {
        // Authentic EXTBG front->back: OBJ3, M7-high, OBJ2, OBJ1, M7-low, OBJ0.
        assert_eq!(
            mode7_extbg_ladder(),
            [
                Slot::Obj { prio: 3 },
                Slot::Bg {
                    layer: 0,
                    prio: true
                },
                Slot::Obj { prio: 2 },
                Slot::Obj { prio: 1 },
                Slot::Bg {
                    layer: 0,
                    prio: false
                },
                Slot::Obj { prio: 0 },
            ]
        );
    }

    /// Build an EXTBG Mode-7 scene: plane pixel col0 = HIGH priority (bit7) color1,
    /// col1 = LOW priority color1. A sprite (yellow) spanning both columns at the
    /// given OBJ prio. Returns the rendered framebuffer.
    fn extbg_scene(obj_prio: u8, extbg: bool) -> Vec<u8> {
        let mut m = Memory::new();
        m.cgram[0] = rgb15(0, 0, 0);
        m.cgram[1] = rgb15(255, 0, 0); // Mode-7 color 1 = red
        m.cgram[128 + 1] = rgb15(255, 255, 0); // OBJ pal0 idx1 = yellow
                                               // Mode-7 tile 0 (map cells default to tile 0): char (0,0)=0x81 hi+color1,
                                               // char (1,0)=0x01 lo+color1. HIGH byte = char lane, LOW byte = map (0=tile0).
        m.vram[0] = 0x81 << 8;
        m.vram[1] = 0x01 << 8;
        // Sprite char 1 covering screen x=0 and x=1 (4bpp plane0 row0 bits 7,6).
        m.obsel.char_base = 0x4000;
        m.vram[0x4000 + 16] = 0x00c0;
        m.oam[0] = Obj {
            on: true,
            x: 0,
            y: 0,
            tile: 1,
            pal: 0,
            prio: obj_prio,
            ..Obj::default()
        };
        let mut src = LineTableRow::default();
        src.mode = 7;
        if extbg {
            src.setini = 0x40;
        }
        let lt = LineTableBuilder::new(src).build(HEIGHT);
        render_frame(&lt, &m)
    }

    #[test]
    fn extbg_sprite_renders_between_mode7_priority_levels() {
        // OBJ prio 2 sits BELOW the high floor but ABOVE the low floor.
        let fb = extbg_scene(2, true);
        assert_eq!(
            &fb[0..4],
            &unpack_rgb15(rgb15(255, 0, 0)),
            "col0 high floor beats OBJ2"
        );
        assert_eq!(
            &fb[4..8],
            &unpack_rgb15(rgb15(255, 255, 0)),
            "col1 OBJ2 beats low floor"
        );
    }

    #[test]
    fn extbg_obj_prio0_sinks_below_both_floor_levels() {
        // OBJ prio 0 is the back rung: both floor pixels win.
        let fb = extbg_scene(0, true);
        assert_eq!(&fb[0..4], &unpack_rgb15(rgb15(255, 0, 0))); // high floor
        assert_eq!(&fb[4..8], &unpack_rgb15(rgb15(255, 0, 0))); // low floor wins over OBJ0
    }

    #[test]
    fn extbg_off_keeps_flat_sprite_overlay() {
        // EXTBG off: sprites ALWAYS overlay the single Mode-7 floor regardless of
        // OBJ priority (no interleave) — today's behavior, unchanged.
        let fb = extbg_scene(0, false);
        assert_eq!(
            &fb[0..4],
            &unpack_rgb15(rgb15(255, 255, 0)),
            "sprite overlays col0"
        );
        assert_eq!(
            &fb[4..8],
            &unpack_rgb15(rgb15(255, 255, 0)),
            "sprite overlays col1"
        );
    }

    #[test]
    fn extbg_with_direct_color_decodes_low7_bits_and_keeps_priority() {
        // Integration reconciliation (M8): with EXTBG *and* direct colour both on,
        // render_mode7_scanline_px expands the pixel's low 7 bits through
        // direct_color_bgr555 (bit 7 still priority). The floor colour therefore does
        // NOT come from CGRAM — leave cgram[1] unset to prove the bypass. OBJ is unaffected.
        let mut m = Memory::new();
        m.cgram[128 + 1] = rgb15(255, 255, 0); // OBJ pal0 idx1 = yellow
        m.vram[0] = 0x81 << 8; // col0: bit7 high + colour index 1
        m.vram[1] = 0x01 << 8; // col1: low + colour index 1
        m.obsel.char_base = 0x4000;
        m.vram[0x4000 + 16] = 0x00c0; // sprite char covering screen x=0,1
        m.oam[0] = Obj {
            on: true,
            x: 0,
            y: 0,
            tile: 1,
            pal: 0,
            prio: 2,
            ..Obj::default()
        };
        let mut src = LineTableRow::default();
        src.mode = 7;
        src.setini = 0x40; // EXTBG
        src.cgwsel = 0x01; // direct colour
        let lt = LineTableBuilder::new(src).build(HEIGHT);
        let fb = render_frame(&lt, &m);
        // col0: high floor beats OBJ2 -> low-7-bits (index 1) through direct colour, not CGRAM.
        // direct_color_bgr555(1, 0): r5 = (1 & 7) << 2 = 4, g5 = 0, b5 = 0 -> raw 0x0004.
        assert_eq!(
            &fb[0..4],
            &unpack_rgb15(0x0004),
            "col0 = direct-colour decode of the low-7 bits (CGRAM bypassed)"
        );
        // col1: OBJ2 beats the low floor -> yellow sprite (OBJ still uses CGRAM).
        assert_eq!(
            &fb[4..8],
            &unpack_rgb15(rgb15(255, 255, 0)),
            "col1 = sprite over the low floor"
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
        let mut s = vec![PixelSource::Backdrop; WIDTH];
        let obj = crate::sprite::sprites_on_line(&m, 0);
        composite_screen(&row, &m, 0, row.tm, row.tmw, &obj, &mut main, &mut s);
        composite_screen(&row, &m, 0, row.ts, row.tsw, &obj, &mut sub, &mut s);
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
        let mut s = vec![PixelSource::Backdrop; WIDTH];
        let obj = crate::sprite::sprites_on_line(&m, 0);
        composite_screen(&row, &m, 0, row.tm, row.tmw, &obj, &mut main, &mut s);
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
        let mut s = vec![PixelSource::Backdrop; WIDTH];
        let obj = crate::sprite::sprites_on_line(&m, 0);
        composite_screen(&row, &m, 0, row.tm, row.tmw, &obj, &mut main, &mut s);
        composite_screen(&row, &m, 0, row.ts, row.tsw, &obj, &mut sub, &mut s);
        // Main screen: no TMW -> BG1 red everywhere.
        assert_eq!(main[0], unpack_rgb15(rgb15(255, 0, 0)));
        // Sub screen: BG1 clipped inside window -> backdrop; visible outside.
        assert_eq!(sub[0], unpack_rgb15(rgb15(0, 0, 0)));
        assert_eq!(sub[8], unpack_rgb15(rgb15(255, 0, 0)));
    }

    #[test]
    fn composite_screen_reports_pixel_sources() {
        // BG1 index1 (red) at (0,0); a sprite (pal 5) at (1,0); backdrop elsewhere.
        let mut m = Memory::new();
        m.cgram[0] = rgb15(0, 0, 0);
        m.cgram[1] = rgb15(255, 0, 0); // BG1 pal0 idx1
        m.cgram[128 + 5 * 16 + 1] = rgb15(0, 255, 0); // OBJ pal5 idx1
        put_px(&mut m, 0x1000, 1); // BG1 char 1 pixel (0,0)
        m.vram[0x0000] = 1;
        m.obsel.char_base = 0x4000;
        m.vram[0x4000 + 16] = 0x0080; // OBJ char 1 pixel (0,0)
        m.oam[0] = Obj {
            on: true,
            x: 1,
            y: 0,
            tile: 1,
            pal: 5,
            prio: 3,
            ..Obj::default()
        };
        let mut src = LineTableRow::default();
        src.bg[0].char_base = 0x1000;
        let row = RegRow::from(&src);
        let mut line = vec![[0u8; 4]; WIDTH];
        let mut srcs = vec![PixelSource::Backdrop; WIDTH];
        let obj = crate::sprite::sprites_on_line(&m, 0);
        composite_screen(&row, &m, 0, row.tm, row.tmw, &obj, &mut line, &mut srcs);
        assert_eq!(srcs[0], PixelSource::Bg(0));
        assert_eq!(srcs[1], PixelSource::Obj { pal: 5 });
        assert_eq!(srcs[2], PixelSource::Backdrop);
        // math_layer() maps sources to CGADSUB bit indices.
        assert_eq!(PixelSource::Bg(0).math_layer(), 0);
        assert_eq!(PixelSource::Obj { pal: 5 }.math_layer(), 4);
        assert_eq!(PixelSource::Backdrop.math_layer(), 5);
    }

    /// Build a two-BG scene: BG1 (pal0) on main, BG2 (pal1) on sub, sharing char 1.
    /// Returns (memory, source-row) with color-math registers left default.
    fn two_screen_scene(bg1: u16, bg2: u16) -> (Memory, LineTableRow) {
        let mut m = Memory::new();
        m.cgram[0] = rgb15(0, 0, 0);
        m.cgram[1] = bg1; // BG1 pal0 idx1
        m.cgram[16 + 1] = bg2; // BG2 pal1 idx1
        put_px(&mut m, 0x1000, 1); // shared char 1
        m.vram[0x0000] = 1; // BG1 map(0,0): tile1 pal0
        m.vram[0x0400] = 1 | (1 << 10); // BG2 map(0,0): tile1 pal1
        let mut src = LineTableRow::default();
        src.bg[0].char_base = 0x1000;
        src.bg[1].char_base = 0x1000;
        src.bg[1].map_base = 0x0400;
        src.tm = 0x01; // main: BG1 only
        src.ts = 0x02; // sub: BG2 only
        (m, src)
    }

    #[test]
    fn half_add_is_clean_translucency() {
        // A = (31,0,0), B = (0,0,31). ½-add -> (15,0,15) in 5-bit.
        let (m, mut src) = two_screen_scene(rgb15(255, 0, 0), rgb15(0, 0, 255));
        src.cgadsub = 0x40 | 0x01; // add + half + BG1 enable
        src.cgwsel = 0x02; // addend = subscreen
        let lt = LineTableBuilder::new(src).build(HEIGHT);
        let want = unpack_rgb15((15 << 10) | 15); // b=15, r=15
        assert_eq!(&render_frame(&lt, &m)[0..4], &want);
    }

    #[test]
    fn subtract_darkens_main_by_sub() {
        // main r=20, sub r=5 -> 15 (no half).
        let (m, mut src) = two_screen_scene(20u16, 5u16);
        src.cgadsub = 0x80 | 0x01; // subtract + BG1 enable
        src.cgwsel = 0x02;
        let lt = LineTableBuilder::new(src).build(HEIGHT);
        assert_eq!(&render_frame(&lt, &m)[0..4], &unpack_rgb15(15));
    }

    #[test]
    fn math_disabled_layer_passes_main_through() {
        // BG1 enable bit clear -> no math; main shows A unchanged.
        let (m, mut src) = two_screen_scene(rgb15(255, 0, 0), rgb15(0, 0, 255));
        src.cgadsub = 0x40 | 0x02; // add + half but only BG2 enabled (not the main source)
        src.cgwsel = 0x02;
        let lt = LineTableBuilder::new(src).build(HEIGHT);
        assert_eq!(
            &render_frame(&lt, &m)[0..4],
            &unpack_rgb15(rgb15(255, 0, 0))
        );
    }

    #[test]
    fn fixed_color_source_uses_coldata_not_subscreen() {
        // addend = fixed color; COLDATA blue=31. main red -> add -> (31,0,31).
        let (m, mut src) = two_screen_scene(rgb15(255, 0, 0), rgb15(0, 255, 0));
        src.cgadsub = 0x01; // add + BG1 enable (no half)
        src.cgwsel = 0x00; // addend = fixed color (bit1 clear)
        src.coldata = 31 << 10; // blue = 31
        let lt = LineTableBuilder::new(src).build(HEIGHT);
        assert_eq!(&render_frame(&lt, &m)[0..4], &unpack_rgb15((31 << 10) | 31));
    }

    #[test]
    fn half_is_suppressed_when_sub_pixel_is_backdrop() {
        // Sub screen empty (TS=0) -> sub pixel is backdrop -> half NOT applied.
        let (m, mut src) = two_screen_scene(rgb15(255, 0, 0), rgb15(0, 0, 255));
        src.ts = 0x00; // sub = backdrop (black) everywhere
        src.cgadsub = 0x40 | 0x01; // add + half + BG1
        src.cgwsel = 0x02; // addend = subscreen (which is backdrop here)
        let lt = LineTableBuilder::new(src).build(HEIGHT);
        // main(31,0,0) + backdrop(0,0,0) = (31,0,0), half suppressed -> unchanged.
        assert_eq!(
            &render_frame(&lt, &m)[0..4],
            &unpack_rgb15(rgb15(255, 0, 0))
        );
    }

    #[test]
    fn backdrop_as_main_source_participates_in_math() {
        // Main screen shows only the backdrop (red); CGADSUB backdrop-enable
        // (bit5) lets the backdrop take part in color math. Sub = BG2 blue.
        let (mut m, mut src) = two_screen_scene(rgb15(0, 255, 0), rgb15(0, 0, 255));
        m.cgram[0] = rgb15(255, 0, 0); // backdrop red
        src.tm = 0x00; // main: nothing -> backdrop shows through
        src.cgadsub = 0x20; // add + backdrop enable (bit5)
        src.cgwsel = 0x02; // addend = subscreen
        let lt = LineTableBuilder::new(src).build(HEIGHT);
        // backdrop(31,0,0) + sub blue(0,0,31) = (31,0,31).
        assert_eq!(&render_frame(&lt, &m)[0..4], &unpack_rgb15((31 << 10) | 31));
    }

    /// two_screen_scene + a color window [0,7] on window 1, enabled for the
    /// COLOR window (WOBJSEL high nibble W1 enable = bit5). Caller sets clip /
    /// prevent modes via cgwsel bits 6-7 / 4-5. `two_screen_scene`'s shared char
    /// only colors its own local pixel (0,0) (see `put_px`), so a second tile
    /// column is mapped in for both BG1 and BG2 — otherwise x=8 (the "outside
    /// window" probe column) would just be backdrop, not a real BG pixel.
    fn color_window_scene(bg1: u16, bg2: u16) -> (Memory, LineTableRow) {
        let (mut m, mut src) = two_screen_scene(bg1, bg2);
        m.vram[0x0001] = 1; // BG1 map tile-column 1 (x=8..15): tile1 pal0
        m.vram[0x0400 + 1] = 1 | (1 << 10); // BG2 map tile-column 1: tile1 pal1
        src.wh0 = 0;
        src.wh1 = 7; // window 1 = columns 0..=7
        src.wobjsel = 0x20; // COLOR window high nibble: W1 enable (bit1 of high nibble)
        (m, src)
    }

    #[test]
    fn clip_to_black_forces_main_black_and_suppresses_half() {
        // clip = inside color window (mode 2); add + half + BG1; addend subscreen.
        let (m, mut src) = color_window_scene(rgb15(255, 0, 0), rgb15(0, 0, 255));
        src.cgadsub = 0x40 | 0x01; // add + half + BG1
        src.cgwsel = 0x02 | (0b10 << 6); // addend=subscreen, clip=inside(10)
        let lt = LineTableBuilder::new(src).build(HEIGHT);
        let fb = render_frame(&lt, &m);
        // Inside window (x=0): main clipped to black -> black + sub(0,0,31),
        // half suppressed by clip -> (0,0,31).
        assert_eq!(&fb[0..4], &unpack_rgb15(31 << 10));
        // Outside window (x=8): normal ½-add -> (15,0,15).
        let o = 8 * 4;
        assert_eq!(&fb[o..o + 4], &unpack_rgb15((15 << 10) | 15));
    }

    #[test]
    fn prevent_math_region_leaves_main_untouched() {
        // prevent = inside window (mode 2); add + BG1; addend subscreen.
        let (m, mut src) = color_window_scene(rgb15(255, 0, 0), rgb15(0, 0, 255));
        src.cgadsub = 0x01; // add + BG1 (no half)
        src.cgwsel = 0x02 | (0b10 << 4); // addend=subscreen, prevent=inside(10)
        let lt = LineTableBuilder::new(src).build(HEIGHT);
        let fb = render_frame(&lt, &m);
        // Inside window (x=0): math prevented -> main red unchanged.
        assert_eq!(&fb[0..4], &unpack_rgb15(rgb15(255, 0, 0)));
        // Outside window (x=8): add applies -> (31,0,31).
        let o = 8 * 4;
        assert_eq!(&fb[o..o + 4], &unpack_rgb15((31 << 10) | 31));
    }

    #[test]
    fn clip_to_black_with_math_disabled_still_blackens() {
        // clip=always, but BG1 math disabled -> pixel is just black inside region.
        let (m, mut src) = two_screen_scene(rgb15(255, 0, 0), rgb15(0, 0, 255));
        src.cgadsub = 0x00; // no layer enabled
        src.cgwsel = 0b11 << 6; // clip = always
        let lt = LineTableBuilder::new(src).build(HEIGHT);
        assert_eq!(&render_frame(&lt, &m)[0..4], &[0, 0, 0, 255]);
    }

    #[test]
    fn obj_palette_gate_excludes_low_palettes() {
        // A sprite on the MAIN screen; sub BG2 = blue via subscreen; add.
        // OBJ math enabled only if sprite uses palette 4-7.
        let build = |pal: u8| {
            let mut m = Memory::new();
            m.cgram[0] = rgb15(0, 0, 0);
            m.cgram[128 + (pal as usize) * 16 + 1] = rgb15(255, 0, 0); // sprite red
            m.cgram[16 + 1] = rgb15(0, 0, 255); // BG2 pal1 blue (sub)
            put_px(&mut m, 0x1000, 1);
            m.vram[0x0400] = 1 | (1 << 10);
            m.obsel.char_base = 0x4000;
            m.vram[0x4000 + 16] = 0x0080;
            m.oam[0] = Obj {
                on: true,
                x: 0,
                y: 0,
                tile: 1,
                pal,
                prio: 3,
                ..Obj::default()
            };
            let mut src = LineTableRow::default();
            src.bg[1].char_base = 0x1000;
            src.bg[1].map_base = 0x0400;
            src.tm = 0x10; // main: OBJ only
            src.ts = 0x02; // sub: BG2 only
            src.cgadsub = 0x10; // add + OBJ enable
            src.cgwsel = 0x02; // addend = subscreen
            let lt = LineTableBuilder::new(src).build(HEIGHT);
            render_frame(&lt, &m)[0..4].to_vec()
        };
        // pal 3: gate blocks math -> sprite red unchanged.
        assert_eq!(build(3), unpack_rgb15(rgb15(255, 0, 0)));
        // pal 4: math applies -> red + blue = (31,0,31).
        assert_eq!(build(4), unpack_rgb15((31 << 10) | 31));
    }

    #[test]
    fn render_frame_stats_reports_overflow_and_main_sub_share_the_set() {
        // 40 opaque 8x8 sprites stacked on the same pixel column at the top-left.
        let mut m = Memory::new();
        m.obsel.char_base = 0x2000;
        m.cgram[0] = rgb15(0, 0, 0);
        m.cgram[128 + 1] = rgb15(255, 0, 0); // OBJ pal0 idx1 = red
                                             // Tile 1 char at 0x2000 + 1*16; plane0 row0 bit7 (0x0080) -> index 1 at x=0.
        m.vram[0x2000 + 16] = 0x0080;
        for i in 0..40usize {
            m.oam[i] = Obj {
                on: true,
                x: 0,
                y: 0,
                tile: 1,
                pal: 0,
                ..Obj::default()
            };
        }
        // OBJ on BOTH screens: default row has OBJ on main (tm bit4); add it to sub.
        let mut def = LineTableRow::default();
        def.ts = 0x10; // OBJ on the sub screen
        let lt = LineTableBuilder::new(def).build(HEIGHT);
        let (fb, ov) = render_frame_stats(&lt, &m);
        assert_eq!(fb.len(), WIDTH * HEIGHT * 4);
        assert!(ov.range_over); // 40 > 32 on line 0
        assert_eq!(ov.max_sprites, 40);
        assert!(!ov.time_over); // 32 kept * 1 sliver < 34
                                // The top-left pixel is a kept sprite (red) on both screens' shared set.
        assert_eq!(&fb[0..4], &unpack_rgb15(rgb15(255, 0, 0)));
        // render_frame is exactly the framebuffer half of render_frame_stats.
        assert_eq!(render_frame(&lt, &m), fb);
    }

    #[test]
    fn force_blank_line_outputs_black_over_bright_content() {
        // BG1 index 1 = red across the row, brightness 15 -> visible red normally.
        let mut def = LineTableRow::default();
        def.force_blank = true;
        let mut m = Memory::new();
        m.cgram[1] = rgb15(255, 0, 0);
        // put a solid BG1 tile so a non-blanked line would be red
        for w in 0..8 {
            m.vram[w] = 0x00ff;
        } // char row plane bits -> index 1s
        let lt = LineTableBuilder::new(def).build(HEIGHT);
        let fb = render_frame(&lt, &m);
        // Every pixel of line 0 is opaque black despite bright red BG content.
        assert_eq!(&fb[0..4], &[0, 0, 0, 255]);
        assert_eq!(&fb[(WIDTH - 1) * 4..(WIDTH - 1) * 4 + 4], &[0, 0, 0, 255]);
    }

    #[test]
    fn render_frame_view_framebuffer_is_byte_identical_to_render_frame() {
        let (m, mut src) = two_screen_scene(rgb15(255, 0, 0), rgb15(0, 0, 255));
        src.cgadsub = 0x40 | 0x01; // the M6 half-add scene
        src.cgwsel = 0x02;
        let lt = LineTableBuilder::new(src).build(HEIGHT);
        let view = render_frame_view(&lt, &m);
        assert_eq!(view.framebuffer, render_frame(&lt, &m));
    }

    #[test]
    fn view_exposes_pre_math_main_and_sub_composites() {
        // half-add: fb blends to (15,0,15) but the intermediates keep the raw screens.
        let (m, mut src) = two_screen_scene(rgb15(255, 0, 0), rgb15(0, 0, 255));
        src.cgadsub = 0x40 | 0x01;
        src.cgwsel = 0x02;
        let lt = LineTableBuilder::new(src).build(HEIGHT);
        let view = render_frame_view(&lt, &m);
        assert_eq!(&view.main[0..4], &unpack_rgb15(rgb15(255, 0, 0))); // BG1 pre-math
        assert_eq!(&view.sub[0..4], &unpack_rgb15(rgb15(0, 0, 255))); // BG2 sub screen
        assert_eq!(view.math_mask[0] & 1, 1); // math applied here
        assert_eq!(&view.framebuffer[0..4], &unpack_rgb15((15 << 10) | 15));
    }

    #[test]
    fn view_screens_are_pre_brightness() {
        let (m, mut src) = two_screen_scene(rgb15(255, 0, 0), rgb15(0, 0, 255));
        src.brightness = 0; // fb black, intermediates un-attenuated
        let lt = LineTableBuilder::new(src).build(HEIGHT);
        let view = render_frame_view(&lt, &m);
        assert_eq!(&view.framebuffer[0..4], &[0, 0, 0, 255]);
        assert_eq!(&view.main[0..4], &unpack_rgb15(rgb15(255, 0, 0)));
    }

    #[test]
    fn math_mask_flags_applied_clip_and_prevent_regions() {
        // prevent = inside window (bits 4-5 = 10): inside -> bit2 set + bit0 clear.
        let (m, mut src) = color_window_scene(rgb15(255, 0, 0), rgb15(0, 0, 255));
        src.cgadsub = 0x01;
        src.cgwsel = 0x02 | (0b10 << 4);
        let lt = LineTableBuilder::new(src).build(HEIGHT);
        let view = render_frame_view(&lt, &m);
        assert_eq!(view.math_mask[0], 0b100); // prevent active, no math
        assert_eq!(view.math_mask[8], 0b001); // outside: math applied
                                              // clip = inside (bits 6-7 = 10): bit1 set inside; math still applies there.
        let (m, mut src) = color_window_scene(rgb15(255, 0, 0), rgb15(0, 0, 255));
        src.cgadsub = 0x01;
        src.cgwsel = 0x02 | (0b10 << 6);
        let lt = LineTableBuilder::new(src).build(HEIGHT);
        let view = render_frame_view(&lt, &m);
        assert_eq!(view.math_mask[0], 0b011); // clip region + math applied
        assert_eq!(view.math_mask[8], 0b001);
    }

    #[test]
    fn math_mask_reports_clip_region_with_math_disabled() {
        // clip=always but no CGADSUB layer enabled: the pixel is blackened via
        // the no-math early return -> bit1 (clip) set, bit0 (applied) clear.
        let (m, mut src) = two_screen_scene(rgb15(255, 0, 0), rgb15(0, 0, 255));
        src.cgadsub = 0x00;
        src.cgwsel = 0b11 << 6;
        let lt = LineTableBuilder::new(src).build(HEIGHT);
        let view = render_frame_view(&lt, &m);
        assert_eq!(view.math_mask[0], 0b010);
        assert_eq!(&view.framebuffer[0..4], &[0, 0, 0, 255]);
    }

    #[test]
    fn force_blank_line_blanks_view_buffers() {
        let (m, mut src) = two_screen_scene(rgb15(255, 0, 0), rgb15(0, 0, 255));
        src.force_blank = true;
        let lt = LineTableBuilder::new(src).build(HEIGHT);
        let view = render_frame_view(&lt, &m);
        assert_eq!(&view.main[0..4], &[0, 0, 0, 255]);
        assert_eq!(&view.sub[0..4], &[0, 0, 0, 255]);
        assert_eq!(view.math_mask[0], 0);
    }
}
