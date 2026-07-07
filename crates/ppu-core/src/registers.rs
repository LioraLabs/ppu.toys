//! Phase-1 register model. `LineTableRow` is the per-scanline (HDMA-able)
//! register state; `Obj` entries are the frame-global sprites stored in OAM.

use crate::quantize;

/// One background layer. `source` names an uploaded image asset (importer sugar
/// from m4/importer + m4/dsl); the binding registers below bind the layer to real VRAM.
#[derive(Clone, Debug, PartialEq)]
pub struct Bg {
    pub scroll_x: f32,
    pub scroll_y: f32,
    pub source: Option<String>,
    pub visible: bool,
    /// Tile size in pixels, 8 or 16 (BGMODE bits 4-7).
    pub tile_size: u8,
    /// Tilemap base as a VRAM word address (BGnSC bits 2-7, 0x400-word steps).
    pub map_base: u32,
    /// Screen size selector 0..3 = 32x32/64x32/32x64/64x64 (BGnSC bits 0-1).
    pub screen_size: u8,
    /// Char (tile data) base as a VRAM word address (BG12/34NBA, 0x1000-word steps).
    pub char_base: u32,
}

impl Default for Bg {
    fn default() -> Self {
        Bg {
            scroll_x: 0.0,
            scroll_y: 0.0,
            source: None,
            visible: true,
            tile_size: 8,
            map_base: 0,
            screen_size: 0,
            char_base: 0,
        }
    }
}

/// Mode 7 affine matrix + rotation/scale center + M7SEL bindings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mode7 {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub cx: f32,
    pub cy: f32,
    /// M7SEL bits 6-7 screen-over: 0/1 = wrap, 2 = transparent, 3 = tile-0 fill.
    pub repeat: u8,
    /// M7SEL bit 0: horizontal flip of the whole 1024x1024 plane.
    pub flip_x: bool,
    /// M7SEL bit 1: vertical flip.
    pub flip_y: bool,
}

impl Default for Mode7 {
    fn default() -> Self {
        // Identity transform, origin (0, 0), wrap, no flip.
        Mode7 {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            cx: 0.0,
            cy: 0.0,
            repeat: 0,
            flip_x: false,
            flip_y: false,
        }
    }
}

/// One sprite (OAM entry). `tile` indexes the global `obj.sheet`. Coordinates are
/// absolute registers: `x` 9-bit signed (negatives run off-left), `y` 8-bit.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Obj {
    pub x: i16,
    pub y: u8,
    pub tile: u16,
    pub pal: u8,  // 0..7: selects the OBJ CGRAM sub-palette (cgram[128 + pal*16 + index])
    pub prio: u8, // 0..3: sprite priority, carried to the compositor
    pub size: u8, // sprite size selector -> sprite_dim (8/16/32/64)
    pub flip_x: bool,
    pub flip_y: bool,
    pub on: bool,
}

/// Frame-global OBJ binding registers (OBSEL $2101). `char_base` is the OBJ
/// tile-data base as an EFFECTIVE VRAM word address (name base, bits 0-2);
/// `size_sel` is the sprite-size pair selector (bits 5-7). Quantize-on-write,
/// consistent with the BG binding registers — the DSL authors friendly values
/// and `lua::read_memory` snaps them via `quantize::obj_char_base`/`obj_size_sel`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Obsel {
    /// Snapped OBJ char base VRAM word address (multiple of 0x2000, in-VRAM).
    pub char_base: u16,
    /// Sprite-size selector, masked to 0..7. Modeled and quantized for register
    /// fidelity; the sampler currently sizes sprites from per-OAM `Obj::size`, so
    /// this is not yet consumed by rendering (forward-looking).
    pub size_sel: u8,
}

/// The effective, resolved register state for a single scanline (one of 224).
/// Also serves as the frame-wide default row from which resolution starts.
#[derive(Clone, Debug, PartialEq)]
pub struct LineTableRow {
    pub mode: u8,       // 0..7
    pub brightness: u8, // 0..15
    pub bg: [Bg; 4],    // bg[1..4] in the DSL -> indices 0..3 here
    pub m7: Mode7,
    /// BGMODE bit 3: BG3-priority, lifts BG3 above BG1/BG2 in Mode 1.
    pub bg3_priority: bool,
}

impl Default for LineTableRow {
    fn default() -> Self {
        LineTableRow {
            mode: 1,
            brightness: 15,
            bg: std::array::from_fn(|_| Bg::default()),
            m7: Mode7::default(),
            bg3_priority: false,
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
    /// Quantized tile size in pixels: exactly 8 or 16.
    pub tile_size: u8,
    /// Snapped tilemap base VRAM word address (multiple of 0x400, in-VRAM).
    pub map_base: u16,
    /// Screen size selector, masked to 0..3.
    pub screen_size: u8,
    /// Snapped char base VRAM word address (multiple of 0x1000, in-VRAM).
    pub char_base: u16,
    /// Bits per pixel this layer renders at in the row's mode, resolved from
    /// the mode table (modes.rs) at quantize time; 0 = the layer does not
    /// exist in this mode (renders transparent).
    pub bpp: u8,
}

impl From<&Bg> for RegBg {
    fn from(b: &Bg) -> Self {
        RegBg {
            scroll_x: quantize::scroll_reg(b.scroll_x),
            scroll_y: quantize::scroll_reg(b.scroll_y),
            source: b.source.clone(),
            visible: b.visible,
            tile_size: quantize::bg_tile_size(b.tile_size),
            map_base: quantize::bg_map_base(b.map_base),
            screen_size: quantize::bg_screen_size(b.screen_size),
            char_base: quantize::bg_char_base(b.char_base),
            bpp: 0, // resolved from the mode table by RegRow::from (needs the row's mode)
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
    /// M7SEL screen-over field, masked to 0..3.
    pub repeat: u8,
    pub flip_x: bool,
    pub flip_y: bool,
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
            repeat: quantize::m7_repeat(m.repeat),
            flip_x: m.flip_x,
            flip_y: m.flip_y,
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
    pub bg3_priority: bool,
}

impl From<&LineTableRow> for RegRow {
    fn from(r: &LineTableRow) -> Self {
        let mode = quantize::mode(r.mode);
        let bpp = crate::modes::mode_info(mode).map_or([0; 4], |m| m.bpp);
        RegRow {
            mode,
            brightness: quantize::brightness(r.brightness),
            bg: std::array::from_fn(|i| {
                let mut b = RegBg::from(&r.bg[i]);
                b.bpp = bpp[i];
                b
            }),
            m7: RegM7::from(&r.m7),
            bg3_priority: r.bg3_priority,
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

    #[test]
    fn bg_defaults_include_binding_registers() {
        let b = Bg::default();
        assert_eq!(b.tile_size, 8);
        assert_eq!(b.map_base, 0);
        assert_eq!(b.screen_size, 0);
        assert_eq!(b.char_base, 0);
    }

    #[test]
    fn m7_defaults_include_binding_registers() {
        let m = Mode7::default();
        assert_eq!(m.repeat, 0);
        assert!(!m.flip_x && !m.flip_y);
    }

    #[test]
    fn regbg_bpp_resolved_from_mode_table() {
        // Mode 1: BG1/BG2 4bpp, BG3 2bpp, BG4 absent.
        let reg = RegRow::from(&LineTableRow::default());
        assert_eq!(
            [reg.bg[0].bpp, reg.bg[1].bpp, reg.bg[2].bpp, reg.bg[3].bpp],
            [4, 4, 2, 0]
        );
        // Unshipped mode (mode_info -> None): every layer bpp 0 (renders transparent).
        let mut src = LineTableRow::default();
        src.mode = 0;
        assert!(RegRow::from(&src).bg.iter().all(|b| b.bpp == 0));
        // Mode 7: the table's BG1 row is 8bpp (tile-BG rasterizer ignores it; mode7.rs owns it).
        src.mode = 7;
        assert_eq!(RegRow::from(&src).bg[0].bpp, 8);
    }

    #[test]
    fn bg3_priority_bit_defaults_off_and_round_trips() {
        assert!(!LineTableRow::default().bg3_priority);
        let mut src = LineTableRow::default();
        src.bg3_priority = true;
        assert!(RegRow::from(&src).bg3_priority);
    }

    #[test]
    fn binding_registers_quantize_on_write() {
        let mut src = LineTableRow::default();
        src.bg[0].tile_size = 16;
        src.bg[0].map_base = 0x07ff; // snaps down to 0x0400
        src.bg[0].screen_size = 5; // masks to 1
        src.bg[0].char_base = 0x1fff; // snaps down to 0x1000
        src.m7.repeat = 6; // masks to 2
        src.m7.flip_x = true;
        let reg = RegRow::from(&src);
        assert_eq!(reg.bg[0].tile_size, 16);
        assert_eq!(reg.bg[0].map_base, 0x0400);
        assert_eq!(reg.bg[0].screen_size, 1);
        assert_eq!(reg.bg[0].char_base, 0x1000);
        assert_eq!(reg.m7.repeat, 2);
        assert!(reg.m7.flip_x && !reg.m7.flip_y);
        // untouched layers keep quantized defaults
        assert_eq!(reg.bg[1].tile_size, 8);
        assert_eq!(reg.bg[1].map_base, 0);
    }
}
