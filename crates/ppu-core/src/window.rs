//! Window masking evaluator — shared infra for TMW/TSW layer clipping and the
//! color-math window. Pure per-x geometry: given a layer's window-select config
//! and the two shared window ranges (WH0-3), decide whether screen column `x` is
//! "inside the combined window". No PPU state, no I/O — so the color-math
//! feature reuses `in_window` verbatim with the COLOR window's bits.
//!
//! `window_scanline_bytes` is the one exception: a read-back seam over a
//! resolved `LineTable` for the Windows editor panel, the window counterpart of
//! `mode7_scanline_segments`.

use crate::linetable::LineTable;

/// The 2-bit WLOG logic combining window 1 and window 2 (WBGLOG/WOBJLOG fields).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WLog {
    Or,
    And,
    Xor,
    Xnor,
}

impl WLog {
    /// Decode a 2-bit WLOG field: 0=OR, 1=AND, 2=XOR, 3=XNOR.
    pub fn from_bits(bits: u8) -> WLog {
        match bits & 0x03 {
            0 => WLog::Or,
            1 => WLog::And,
            2 => WLog::Xor,
            _ => WLog::Xnor,
        }
    }

    fn apply(self, a: bool, b: bool) -> bool {
        match self {
            WLog::Or => a | b,
            WLog::And => a & b,
            WLog::Xor => a ^ b,
            WLog::Xnor => !(a ^ b),
        }
    }
}

/// The two shared window ranges from WH0-3. Inclusive `[left, right]`; `left > right`
/// is an empty range (the raw range test is false for every x).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowRanges {
    pub w1_left: u8,
    pub w1_right: u8,
    pub w2_left: u8,
    pub w2_right: u8,
}

/// One layer's (or the color window's) window-select config: which of window 1/2
/// apply, each optionally inverted, and how the two combine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowSel {
    pub w1_enable: bool,
    pub w1_invert: bool,
    pub w2_enable: bool,
    pub w2_invert: bool,
    pub logic: WLog,
}

impl WindowSel {
    /// Decode a window-select nibble (W12SEL/W34SEL/WOBJSEL low or high nibble)
    /// plus a 2-bit WLOG field. Nibble layout (LSB first): W1 invert, W1 enable,
    /// W2 invert, W2 enable.
    pub fn from_bits(sel_nibble: u8, logic_bits: u8) -> WindowSel {
        WindowSel {
            w1_invert: sel_nibble & 0x01 != 0,
            w1_enable: sel_nibble & 0x02 != 0,
            w2_invert: sel_nibble & 0x04 != 0,
            w2_enable: sel_nibble & 0x08 != 0,
            logic: WLog::from_bits(logic_bits),
        }
    }
}

/// Is column `x` (0..=255) inside `sel`'s combined window over ranges `win`?
/// Hardware semantics: each window's raw inclusive range test (empty range =
/// false), optionally inverted; a disabled window drops out so only the enabled
/// window applies; with neither enabled the result is false (no clip); with both
/// enabled the WLOG op combines them.
pub fn in_window(sel: &WindowSel, win: &WindowRanges, x: usize) -> bool {
    let x = x as u8;
    let m1 = sel.w1_invert ^ (win.w1_left <= x && x <= win.w1_right);
    let m2 = sel.w2_invert ^ (win.w2_left <= x && x <= win.w2_right);
    match (sel.w1_enable, sel.w2_enable) {
        (false, false) => false,
        (true, false) => m1,
        (false, true) => m2,
        (true, true) => sel.logic.apply(m1, m2),
    }
}

/// Bytes per row in `window_scanline_bytes`, and their order: WH0, WH1, WH2,
/// WH3, W12SEL, W34SEL, WOBJSEL, WBGLOG, WOBJLOG, TMW, TSW. Deliberately the
/// same layout as the friendly `win` namespace's `WinBytes` (`lua.rs`) and
/// `WIN_SCANLINE_REGS` in the web inspector's compose model.
pub const WIN_SCANLINE_STRIDE: usize = 11;

/// Every scanline's window registers from a resolved frame — the Windows
/// editor panel's per-scanline feed, mirroring `mode7_scanline_segments`.
/// `WIN_SCANLINE_STRIDE` bytes per row, row 0 first, absolute (post-quantize)
/// values. Windows have no "not applicable" state, so unlike the M7 fan there
/// is no NaN row: every scanline reports its effective mask registers, whether
/// an `hdma()` hook moved them or they are still the frame-wide defaults.
pub fn window_scanline_bytes(lt: &LineTable) -> Vec<u8> {
    let mut out = Vec::with_capacity(lt.rows.len() * WIN_SCANLINE_STRIDE);
    for row in &lt.rows {
        out.extend_from_slice(&[
            row.wh0,
            row.wh1,
            row.wh2,
            row.wh3,
            row.w12sel,
            row.w34sel,
            row.wobjsel,
            row.wbglog,
            row.wobjlog,
            row.tmw,
            row.tsw,
        ]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linetable::LineTableBuilder;
    use crate::registers::LineTableRow;

    const FULL: WindowRanges = WindowRanges {
        w1_left: 0,
        w1_right: 255,
        w2_left: 0,
        w2_right: 255,
    };

    fn sel(w1e: bool, w1i: bool, w2e: bool, w2i: bool, logic: WLog) -> WindowSel {
        WindowSel {
            w1_enable: w1e,
            w1_invert: w1i,
            w2_enable: w2e,
            w2_invert: w2i,
            logic,
        }
    }

    #[test]
    fn wlog_from_bits_decodes_all_four() {
        assert_eq!(WLog::from_bits(0), WLog::Or);
        assert_eq!(WLog::from_bits(1), WLog::And);
        assert_eq!(WLog::from_bits(2), WLog::Xor);
        assert_eq!(WLog::from_bits(3), WLog::Xnor);
        assert_eq!(WLog::from_bits(0xff), WLog::Xnor); // masks to 2 bits
    }

    #[test]
    fn windowsel_from_bits_maps_nibble() {
        // nibble 0b1010 = W1 enable + W2 enable, no invert.
        let s = WindowSel::from_bits(0b1010, 0);
        assert!(s.w1_enable && s.w2_enable && !s.w1_invert && !s.w2_invert);
        // nibble 0b0101 = W1 invert + W2 invert, not enabled.
        let s = WindowSel::from_bits(0b0101, 1);
        assert!(s.w1_invert && s.w2_invert && !s.w1_enable && !s.w2_enable);
        assert_eq!(s.logic, WLog::And);
    }

    #[test]
    fn single_window_range_is_inclusive() {
        // Only window 1 enabled, range [64,192].
        let s = sel(true, false, false, false, WLog::Or);
        let win = WindowRanges {
            w1_left: 64,
            w1_right: 192,
            w2_left: 0,
            w2_right: 0,
        };
        assert!(!in_window(&s, &win, 63));
        assert!(in_window(&s, &win, 64)); // left edge inclusive
        assert!(in_window(&s, &win, 128));
        assert!(in_window(&s, &win, 192)); // right edge inclusive
        assert!(!in_window(&s, &win, 193));
    }

    #[test]
    fn disabled_windows_never_clip() {
        let s = sel(false, false, false, false, WLog::Or);
        for x in [0usize, 100, 255] {
            assert!(!in_window(&s, &FULL, x));
        }
    }

    #[test]
    fn inversion_flips_the_range() {
        // Window 1 only, inverted: inside == OUTSIDE [64,192].
        let s = sel(true, true, false, false, WLog::Or);
        let win = WindowRanges {
            w1_left: 64,
            w1_right: 192,
            w2_left: 0,
            w2_right: 0,
        };
        assert!(in_window(&s, &win, 0));
        assert!(!in_window(&s, &win, 128));
        assert!(in_window(&s, &win, 255));
    }

    #[test]
    fn empty_range_masks_nothing_but_inverts_to_everything() {
        // left > right: raw range false everywhere.
        let win = WindowRanges {
            w1_left: 200,
            w1_right: 100,
            w2_left: 0,
            w2_right: 0,
        };
        let s = sel(true, false, false, false, WLog::Or);
        assert!((0..=255).all(|x| !in_window(&s, &win, x)));
        // inverted empty range -> inside everywhere.
        let s = sel(true, true, false, false, WLog::Or);
        assert!((0..=255).all(|x| in_window(&s, &win, x)));
    }

    #[test]
    fn full_width_window_is_always_inside() {
        let s = sel(true, false, false, false, WLog::Or);
        assert!((0..=255).all(|x| in_window(&s, &FULL, x)));
    }

    #[test]
    fn both_windows_combine_under_each_logic_op() {
        // W1 = [0,127], W2 = [64,255]. Overlap [64,127].
        let win = WindowRanges {
            w1_left: 0,
            w1_right: 127,
            w2_left: 64,
            w2_right: 255,
        };
        // sample points: 32 (only W1), 96 (both), 200 (only W2).
        let cases = [
            (WLog::Or, [true, true, true]),     // in either
            (WLog::And, [false, true, false]),  // in both
            (WLog::Xor, [true, false, true]),   // in exactly one
            (WLog::Xnor, [false, true, false]), // in both or neither
        ];
        for (logic, expect) in cases {
            let s = sel(true, false, true, false, logic);
            assert_eq!(in_window(&s, &win, 32), expect[0], "{logic:?} @32");
            assert_eq!(in_window(&s, &win, 96), expect[1], "{logic:?} @96");
            assert_eq!(in_window(&s, &win, 200), expect[2], "{logic:?} @200");
        }
    }

    #[test]
    fn only_enabled_window_applies_regardless_of_logic() {
        // W2 disabled: AND logic must NOT force everything false — result == W1.
        let win = WindowRanges {
            w1_left: 64,
            w1_right: 192,
            w2_left: 0,
            w2_right: 255,
        };
        let s = sel(true, false, false, false, WLog::And);
        assert!(in_window(&s, &win, 128)); // W1 inside; W2 ignored despite AND
        assert!(!in_window(&s, &win, 0));
    }

    #[test]
    fn scanline_bytes_report_one_stride_per_row_in_declared_order() {
        let mut def = LineTableRow::default();
        def.wh0 = 10;
        def.wh1 = 20;
        def.wh2 = 30;
        def.wh3 = 40;
        def.w12sel = 0x02;
        def.w34sel = 0x20;
        def.wobjsel = 0x22;
        def.wbglog = 0x01;
        def.wobjlog = 0x02;
        def.tmw = 0x01;
        def.tsw = 0x02;
        let lt = LineTableBuilder::new(def).build(3);
        let b = window_scanline_bytes(&lt);
        assert_eq!(b.len(), 3 * WIN_SCANLINE_STRIDE);
        let row0 = &b[..WIN_SCANLINE_STRIDE];
        assert_eq!(
            row0,
            [10, 20, 30, 40, 0x02, 0x20, 0x22, 0x01, 0x02, 0x01, 0x02]
        );
        // unhooked rows all carry the frame-wide defaults
        assert_eq!(&b[WIN_SCANLINE_STRIDE..2 * WIN_SCANLINE_STRIDE], row0);
    }

    #[test]
    fn scanline_bytes_follow_a_per_line_hook() {
        // The spotlight iris shape: W1 spans a circle's chord on each row.
        let mut b = LineTableBuilder::new(LineTableRow::default());
        b.hdma(0, 3, |y, r| {
            r.wh0 = 128 - (y as u8);
            r.wh1 = 128 + (y as u8);
        });
        let bytes = window_scanline_bytes(&b.build(4));
        let at = |y: usize, i: usize| bytes[y * WIN_SCANLINE_STRIDE + i];
        assert_eq!((at(0, 0), at(0, 1)), (128, 128));
        assert_eq!((at(3, 0), at(3, 1)), (125, 131));
        // registers the hook never touched stay flat across the frame
        assert!((0..4).all(|y| at(y, 4) == at(0, 4)));
    }
}
