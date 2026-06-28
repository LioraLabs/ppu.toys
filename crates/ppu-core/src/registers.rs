//! Phase-1 register model. `LineTableRow` is the per-scanline (HDMA-able)
//! register state; `Obj` entries are the frame-global sprites stored in OAM.

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
        // Identity transform, origin center.
        Mode7 { a: 1.0, b: 0.0, c: 0.0, d: 1.0, cx: 0.0, cy: 0.0 }
    }
}

/// One sprite (OAM entry). `tile` indexes the global `obj.sheet`.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Obj {
    pub x: f32,
    pub y: f32,
    pub tile: u16,
    pub pal: u8,  // 0..7
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

#[cfg(test)]
mod tests {
    use super::*;

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
