//! Line table: frame-wide defaults + per-scanline override hooks, resolved into
//! 224 `LineTableRow`s. Mirrors the spec frame lifecycle: start from defaults,
//! apply each covering hook in registration order (later call wins).

use crate::registers::{LineTableRow, RegRow};

/// A per-scanline override hook registered for the inclusive line range
/// `[y0, y1]`. The closure pokes the working row exactly as a Lua hook pokes
/// globals; the downstream Lua ticket supplies closures that call into the VM.
pub struct Hook {
    pub y0: usize,
    pub y1: usize,
    pub apply: Box<dyn Fn(usize, &mut LineTableRow)>,
}

/// Accumulates the frame-wide defaults plus registered hooks, then resolves the
/// effective row for each scanline.
pub struct LineTableBuilder {
    pub defaults: LineTableRow,
    pub hooks: Vec<Hook>,
}

impl LineTableBuilder {
    pub fn new(defaults: LineTableRow) -> Self {
        LineTableBuilder {
            defaults,
            hooks: Vec::new(),
        }
    }

    /// Register a hook over the inclusive scanline range `[y0, y1]`. Alias of
    /// the DSL `hdma`/`scanline`. Registration order is preserved.
    pub fn hdma(
        &mut self,
        y0: usize,
        y1: usize,
        apply: impl Fn(usize, &mut LineTableRow) + 'static,
    ) {
        self.hooks.push(Hook {
            y0,
            y1,
            apply: Box::new(apply),
        });
    }

    /// Resolve the effective register state for scanline `y`: start from the
    /// defaults, then apply every hook covering `y` in registration order.
    pub fn resolve(&self, y: usize) -> LineTableRow {
        let mut row = self.defaults.clone();
        for h in &self.hooks {
            if h.y0 <= y && y <= h.y1 {
                (h.apply)(y, &mut row);
            }
        }
        row
    }

    /// Resolve all `height` scanlines, then quantize each to its absolute
    /// register state (quantize-on-write happens HERE, once per line).
    pub fn build(&self, height: usize) -> LineTable {
        let mut rows: Vec<RegRow> = (0..height)
            .map(|y| RegRow::from(&self.resolve(y)))
            .collect();
        // A size change restarts the vertical mosaic count; same-size writes and
        // enable-bit changes do not (bsnes accuracy PPU).
        let mut top = 0;
        for y in 1..rows.len() {
            if rows[y].mosaic_size != rows[y - 1].mosaic_size {
                top = y as u16;
            }
            for b in &mut rows[y].bg {
                b.mosaic_top = top;
            }
        }
        LineTable { rows }
    }
}

/// The resolved per-scanline register state for a whole frame (absolute values).
#[derive(Debug)]
pub struct LineTable {
    pub rows: Vec<RegRow>,
}

/// Deterministic placeholder rasterizer. The real Phase-2 compositor (CGRAM +
/// sources + sprites -> pixels) lands in a later M1 ticket; until then each
/// scanline is filled with a debug color derived from its resolved registers so
/// the resolution -> rasterize pipeline is golden-testable now.
pub fn rasterize(lt: &LineTable, width: usize, height: usize) -> Vec<u8> {
    let mut fb = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        let px = line_debug_color(&lt.rows[y]);
        for _ in 0..width {
            fb.extend_from_slice(&px);
        }
    }
    fb
}

fn line_debug_color(row: &RegRow) -> [u8; 4] {
    [
        row.mode,
        row.brightness,
        (row.bg[0].scroll_x as i64).rem_euclid(256) as u8,
        255,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HEIGHT;

    #[test]
    fn unhooked_line_equals_defaults() {
        let b = LineTableBuilder::new(LineTableRow::default());
        assert_eq!(b.resolve(50), LineTableRow::default());
    }

    #[test]
    fn hook_overrides_only_within_its_range() {
        let mut b = LineTableBuilder::new(LineTableRow::default());
        b.hdma(96, 223, |_, r| r.mode = 7);
        assert_eq!(b.resolve(50).mode, 1); // default, outside range
        assert_eq!(b.resolve(96).mode, 7); // inclusive start
        assert_eq!(b.resolve(223).mode, 7); // inclusive end
    }

    #[test]
    fn hook_value_can_vary_per_scanline() {
        let mut b = LineTableBuilder::new(LineTableRow::default());
        b.hdma(96, 223, |y, r| r.m7.a = (y as f32) - 95.0);
        assert_eq!(b.resolve(96).m7.a, 1.0);
        assert_eq!(b.resolve(100).m7.a, 5.0);
    }

    #[test]
    fn later_hook_wins_on_overlap() {
        let mut b = LineTableBuilder::new(LineTableRow::default());
        b.hdma(0, 223, |_, r| r.brightness = 4);
        b.hdma(0, 223, |_, r| r.brightness = 9); // registered later
        assert_eq!(b.resolve(10).brightness, 9);
    }

    #[test]
    fn per_line_mode_change_is_allowed() {
        // The split-screen unlock: Mode 1 HUD on top, Mode 7 floor below.
        let mut b = LineTableBuilder::new(LineTableRow::default());
        b.hdma(96, 223, |_, r| r.mode = 7);
        assert_eq!(b.resolve(0).mode, 1);
        assert_eq!(b.resolve(120).mode, 7);
    }

    #[test]
    fn build_quantizes_fractional_scroll_to_whole_px() {
        let mut def = LineTableRow::default();
        def.bg[0].scroll_x = 30.7;
        let lt = LineTableBuilder::new(def).build(2);
        assert_eq!(lt.rows[0].bg[0].scroll_x, 31); // stored absolute, rounded
    }

    #[test]
    fn mosaic_blocks_restart_where_the_size_changes() {
        // Measured on bsnes (accuracy PPU): 4-row blocks from row 0, size 2 from
        // row 101 gives blocks at 100, 101, 104, 107; an enable change at row 50
        // and same-size rows do not restart the count.
        let mut def = LineTableRow::default();
        def.mosaic_size = 3;
        def.mosaic_enable[0] = true;
        let mut b = LineTableBuilder::new(def);
        b.hdma(50, 223, |_, r| r.mosaic_enable[1] = true);
        b.hdma(101, 223, |_, r| r.mosaic_size = 2);
        let lt = b.build(HEIGHT);
        assert_eq!(lt.rows[100].bg[0].mosaic_top, 0);
        assert_eq!(lt.rows[101].bg[0].mosaic_top, 101);
        assert_eq!(lt.rows[200].bg[3].mosaic_top, 101);
        assert_eq!(lt.rows[101].bg[0].mosaic, 3);
    }

    #[test]
    fn build_resolves_every_scanline() {
        let mut b = LineTableBuilder::new(LineTableRow::default());
        b.hdma(0, HEIGHT - 1, |y, r| r.brightness = (y % 16) as u8);
        let lt = b.build(HEIGHT);
        assert_eq!(lt.rows.len(), HEIGHT);
        assert_eq!(lt.rows[17].brightness, 1);
    }

    #[test]
    fn rasterize_fills_each_row_from_its_registers() {
        let mut b = LineTableBuilder::new(LineTableRow::default());
        b.hdma(0, 0, |_, r| {
            r.mode = 3;
            r.brightness = 9;
        });
        let lt = b.build(2);
        let fb = rasterize(&lt, 2, 2);
        // row 0: overridden (mode 3, brightness 9); row 1: defaults (1, 15).
        assert_eq!(
            fb,
            vec![3, 9, 0, 255, 3, 9, 0, 255, 1, 15, 0, 255, 1, 15, 0, 255]
        );
    }
}
