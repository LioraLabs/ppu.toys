//! Phase-1 register model. `LineTableRow` is the per-scanline (HDMA-able)
//! register state; `Obj` entries are the frame-global sprites stored in OAM.

use crate::quantize;

/// One background layer. `source` names an uploaded image asset; the engine
/// auto-tiles / scrolls / (in Mode 7) transforms over it.
#[derive(Clone, Debug, PartialEq)]
pub struct Bg {
    pub scroll_x: f32,
    pub scroll_y: f32,
    pub source: Option<String>,
    pub visible: bool,
}

impl Default for Bg {
    fn default() -> Self {
        Bg { scroll_x: 0.0, scroll_y: 0.0, source: None, visible: true }
    }
}

/// Mode 7 affine matrix + rotation/scale center.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mode7 {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub cx: f32,
    pub cy: f32,
}

impl Default for Mode7 {
    fn default() -> Self {
        // Identity transform, origin (0, 0).
        Mode7 { a: 1.0, b: 0.0, c: 0.0, d: 1.0, cx: 0.0, cy: 0.0 }
    }
}

/// One sprite (OAM entry). `tile` indexes the global `obj.sheet`. Coordinates are
/// absolute registers: `x` 9-bit signed (negatives run off-left), `y` 8-bit.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Obj {
    pub x: i16,
    pub y: u8,
    pub tile: u16,
    pub pal: u8,  // 0..7; NO-OP in v1 (direct-RGBA sheets), reserved for v2 per-palette recolor
    pub prio: u8, // 0..3
    pub size: u8, // sprite size selector
    pub flip_x: bool,
    pub flip_y: bool,
    pub on: bool,
}

/// The effective, resolved register state for a single scanline (one of 224).
/// Also serves as the frame-wide default row from which resolution starts.
#[derive(Clone, Debug, PartialEq)]
pub struct LineTableRow {
    pub mode: u8,       // 0..7
    pub brightness: u8, // 0..15
    pub bg: [Bg; 4],    // bg[1..4] in the DSL -> indices 0..3 here
    pub m7: Mode7,
}

impl Default for LineTableRow {
    fn default() -> Self {
        LineTableRow {
            mode: 1,
            brightness: 15,
            bg: std::array::from_fn(|_| Bg::default()),
            m7: Mode7::default(),
        }
    }
}

/// Absolute (quantized) per-layer register state the rasterizer reads. Scroll is
/// whole-pixel; `source`/`visible` are carried through (not registers, but the
/// compositor needs them). No `Default` — always built via `From<&Bg>` (a derived
/// default would give `visible: false`, contradicting `Bg`'s `visible: true`).
#[derive(Clone, Debug, PartialEq)]
pub struct RegBg {
    pub scroll_x: i16,
    pub scroll_y: i16,
    pub source: Option<String>,
    pub visible: bool,
}

impl From<&Bg> for RegBg {
    fn from(b: &Bg) -> Self {
        RegBg {
            scroll_x: quantize::scroll_reg(b.scroll_x),
            scroll_y: quantize::scroll_reg(b.scroll_y),
            source: b.source.clone(),
            visible: b.visible,
        }
    }
}

/// Absolute Mode 7 matrix: a/b/c/d in Q1.7.8 fixed point; cx/cy whole-pixel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RegM7 {
    pub a: i16,
    pub b: i16,
    pub c: i16,
    pub d: i16,
    pub cx: i16,
    pub cy: i16,
}

impl From<&Mode7> for RegM7 {
    fn from(m: &Mode7) -> Self {
        RegM7 {
            a: quantize::m7_matrix(m.a),
            b: quantize::m7_matrix(m.b),
            c: quantize::m7_matrix(m.c),
            d: quantize::m7_matrix(m.d),
            cx: quantize::m7_center(m.cx),
            cy: quantize::m7_center(m.cy),
        }
    }
}

/// The absolute, quantized per-scanline register state — what the LineTable
/// stores, the rasterizer samples, and the inspector shows. Produced from the
/// float authoring `LineTableRow` at `build()` time (quantize-on-write).
#[derive(Clone, Debug, PartialEq)]
pub struct RegRow {
    pub mode: u8,
    pub brightness: u8,
    pub bg: [RegBg; 4],
    pub m7: RegM7,
}

impl From<&LineTableRow> for RegRow {
    fn from(r: &LineTableRow) -> Self {
        RegRow {
            mode: quantize::mode(r.mode),
            brightness: quantize::brightness(r.brightness),
            bg: std::array::from_fn(|i| RegBg::from(&r.bg[i])),
            m7: RegM7::from(&r.m7),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regrow_quantizes_from_authoring_row() {
        let mut src = LineTableRow::default(); // mode 1, brightness 15
        src.bg[0].scroll_x = 10.7;
        src.bg[0].scroll_y = -1.6;
        src.bg[0].source = Some("sky".into());
        src.m7.a = 0.5; // -> Q8 128
        src.m7.cx = 127.6; // -> 128
        let reg = RegRow::from(&src);
        assert_eq!(reg.mode, 1);
        assert_eq!(reg.brightness, 15);
        assert_eq!(reg.bg[0].scroll_x, 11); // rounded whole px
        assert_eq!(reg.bg[0].scroll_y, -2);
        assert_eq!(reg.bg[0].source.as_deref(), Some("sky"));
        assert!(reg.bg[0].visible);
        assert_eq!(reg.m7.a, 128); // Q8
        assert_eq!(reg.m7.cx, 128);
    }

    #[test]
    fn regrow_default_is_identity_mode1_bright15() {
        let reg = RegRow::from(&LineTableRow::default());
        assert_eq!((reg.mode, reg.brightness), (1, 15));
        assert_eq!(reg.m7.a, 256); // identity 1.0 in Q8
        assert_eq!(reg.m7.d, 256);
        assert_eq!(reg.bg[0].scroll_x, 0);
    }

    #[test]
    fn line_row_defaults_match_spec() {
        let r = LineTableRow::default();
        assert_eq!(r.mode, 1);
        assert_eq!(r.brightness, 15);
        assert_eq!(r.bg.len(), 4);
        assert!(r.bg.iter().all(|b| b.visible && b.source.is_none()));
        assert_eq!(r.m7, Mode7::default());
    }

    #[test]
    fn mode7_default_is_identity() {
        let m = Mode7::default();
        assert_eq!((m.a, m.b, m.c, m.d), (1.0, 0.0, 0.0, 1.0));
    }

    #[test]
    fn obj_default_is_off() {
        let o = Obj::default();
        assert!(!o.on && !o.flip_x && !o.flip_y);
        assert_eq!((o.tile, o.pal, o.prio), (0, 0, 0));
    }
}
