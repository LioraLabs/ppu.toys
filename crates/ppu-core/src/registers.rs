//! Phase-1 register model. `LineTableRow` is the per-scanline (HDMA-able)
//! register state; `Obj` entries are the frame-global sprites stored in OAM.

use crate::quantize;
use crate::window::{WindowRanges, WindowSel};

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
    pub large: bool, // OAM high-table size bit: picks small/large from the OBSEL size-pair
    pub flip_x: bool,
    pub flip_y: bool,
    pub on: bool,
}

impl Obj {
    /// The SNES OAM *high table* nibble for this sprite: bit 0 = X bit 8 (the 9th
    /// X bit / sign, set when `x` does not fit in unsigned 8 bits), bit 1 = the
    /// `large` size bit. (Actual OAM serialization packs 4 sprites per byte — S5.)
    pub fn oam_high_bits(&self) -> u8 {
        let x_bit8 = ((self.x as i32) & !0xff) != 0; // outside 0..=255 -> bit 8 set
        (x_bit8 as u8) | ((self.large as u8) << 1)
    }

    /// Reconstruct the size + 9-bit signed X from the low-table X byte and the
    /// high-table nibble (inverse of the OAM split). X is `low_x | (bit8 << 8)`,
    /// interpreted as 9-bit signed (bit 8 = sign).
    pub fn from_oam_high(low_x: u8, high_bits: u8) -> Obj {
        let raw = (low_x as u16) | (((high_bits & 1) as u16) << 8); // 9-bit
        let x = if raw & 0x100 != 0 {
            (raw | 0xfe00) as i16 // sign-extend bit 8
        } else {
            raw as i16
        };
        Obj {
            x,
            large: high_bits & 0b10 != 0,
            ..Obj::default()
        }
    }
}

/// Frame-global OBJ binding registers (OBSEL $2101). `char_base` is the OBJ
/// tile-data base as an EFFECTIVE VRAM word address (name base, bits 0-2);
/// `name_select` is the second name-table gap selector (bits 3-4); `size_sel`
/// is the sprite-size pair selector (bits 5-7). Quantize-on-write, consistent
/// with the BG binding registers — the DSL authors friendly values and
/// `lua::read_memory` snaps them via
/// `quantize::obj_char_base`/`obj_name_select`/`obj_size_sel`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Obsel {
    /// Snapped OBJ char base VRAM word address (multiple of 0x2000, in-VRAM).
    pub char_base: u16,
    /// Name-select (bits 3-4), masked to 0..3: second name-table gap in
    /// 0x1000-word units (second table = char_base + (name_select+1)*0x1000).
    pub name_select: u8,
    /// Sprite-size selector, masked to 0..7 (OBSEL bits 5-7): indexes the
    /// authentic size-pair table (small/large WxH) consumed by the rasterizer.
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
    /// TM ($212C) main-screen layer designation: bits 0-4 = BG1..BG4,OBJ.
    pub tm: u8,
    /// TS ($212D) sub-screen layer designation: same five bits.
    pub ts: u8,
    /// Window 1 left/right edges (WH0 $2126 / WH1 $2127), inclusive on 0..255.
    pub wh0: u8,
    pub wh1: u8,
    /// Window 2 left/right edges (WH2 $2128 / WH3 $2129).
    pub wh2: u8,
    pub wh3: u8,
    /// Per-BG window enable/invert (nibble = W1inv,W1en,W2inv,W2en). W12SEL
    /// ($2123): BG1 low / BG2 high. W34SEL ($2124): BG3 low / BG4 high.
    pub w12sel: u8,
    pub w34sel: u8,
    /// OBJ (low nibble) + COLOR (high nibble) window enable/invert (WOBJSEL $2125).
    pub wobjsel: u8,
    /// 2-bit-per-layer window logic. WBGLOG ($212A): BG1..BG4. WOBJLOG ($212B):
    /// OBJ (bits 0-1) + COLOR (bits 2-3). 0=OR,1=AND,2=XOR,3=XNOR.
    pub wbglog: u8,
    pub wobjlog: u8,
    /// Per-layer "disable inside window" on main (TMW $212E) / sub (TSW $212F);
    /// bits 0-4 = BG1..BG4,OBJ.
    pub tmw: u8,
    pub tsw: u8,
    /// CGWSEL ($2130) color-math control: bit0 direct color, bit1 addend select
    /// (0=fixed color, 1=subscreen), bits4-5 prevent-math region, bits6-7
    /// clip-to-black region. Power-on 0 = no math effect.
    pub cgwsel: u8,
    /// CGADSUB ($2131): bit7 subtract, bit6 half, bits0-5 math-enable per
    /// BG1,BG2,BG3,BG4,OBJ,backdrop. Power-on 0 = math disabled everywhere.
    pub cgadsub: u8,
    /// COLDATA ($2132) fixed color, 15-bit BGR. Power-on 0 = black.
    pub coldata: u16,
}

impl Default for LineTableRow {
    fn default() -> Self {
        LineTableRow {
            mode: 1,
            brightness: 15,
            bg: std::array::from_fn(|_| Bg::default()),
            m7: Mode7::default(),
            bg3_priority: false,
            tm: 0x1f, // all five layers on the main screen (playground full-visibility)
            ts: 0x00, // sub screen empty at power-on
            wh0: 0,
            wh1: 0,
            wh2: 0,
            wh3: 0,
            w12sel: 0,
            w34sel: 0,
            wobjsel: 0,
            wbglog: 0,
            wobjlog: 0,
            tmw: 0,
            tsw: 0,
            cgwsel: 0,
            cgadsub: 0,
            coldata: 0,
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
    /// Resolved row mode this layer belongs to.
    pub mode: u8,
    /// Zero-based BG layer index within the row.
    pub layer: u8,
    /// Quantized tile size in pixels: exactly 8 or 16.
    pub tile_size: u8,
    /// Snapped tilemap base VRAM word address (multiple of 0x400, in-VRAM).
    pub map_base: u16,
    /// Screen size selector, masked to 0..3.
    pub screen_size: u8,
    /// Snapped char base VRAM word address (multiple of 0x1000, in-VRAM).
    pub char_base: u16,
    /// BG3 tilemap base used as the offset-per-tile table in modes 2/4.
    pub offset_map_base: u16,
    /// BG3 screen size selector for the offset table.
    pub offset_screen_size: u8,
    /// Bits per pixel this layer renders at in the row's mode, resolved from
    /// the mode table (modes.rs) at quantize time; 0 = the layer does not
    /// exist in this mode (renders transparent).
    pub bpp: u8,
    /// CGWSEL bit0 direct-color mode: an 8bpp index is read as a direct BGR555
    /// color built from index+palette bits instead of a CGRAM lookup. Only
    /// affects 8bpp layers; ignored otherwise.
    pub direct_color: bool,
}

impl From<&Bg> for RegBg {
    fn from(b: &Bg) -> Self {
        RegBg {
            scroll_x: quantize::scroll_reg(b.scroll_x),
            scroll_y: quantize::scroll_reg(b.scroll_y),
            source: b.source.clone(),
            visible: b.visible,
            mode: 0,
            layer: 0,
            tile_size: quantize::bg_tile_size(b.tile_size),
            map_base: quantize::bg_map_base(b.map_base),
            screen_size: quantize::bg_screen_size(b.screen_size),
            char_base: quantize::bg_char_base(b.char_base),
            offset_map_base: 0,
            offset_screen_size: 0,
            bpp: 0, // resolved from the mode table by RegRow::from (needs the row's mode)
            direct_color: false,
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
    pub tm: u8,
    pub ts: u8,
    pub wh0: u8,
    pub wh1: u8,
    pub wh2: u8,
    pub wh3: u8,
    pub w12sel: u8,
    pub w34sel: u8,
    pub wobjsel: u8,
    pub wbglog: u8,
    pub wobjlog: u8,
    pub tmw: u8,
    pub tsw: u8,
    pub cgwsel: u8,
    pub cgadsub: u8,
    pub coldata: u16,
}

impl From<&LineTableRow> for RegRow {
    fn from(r: &LineTableRow) -> Self {
        let mode = quantize::mode(r.mode);
        let bpp = crate::modes::mode_info(mode).map_or([0; 4], |m| m.bpp);
        let mut bg = std::array::from_fn(|i| {
            let mut b = RegBg::from(&r.bg[i]);
            b.mode = mode;
            b.layer = i as u8;
            b.bpp = bpp[i];
            b.direct_color = r.cgwsel & 1 != 0;
            b
        });
        let offset_map_base = bg[2].map_base;
        let offset_screen_size = bg[2].screen_size;
        for b in &mut bg {
            b.offset_map_base = offset_map_base;
            b.offset_screen_size = offset_screen_size;
        }
        RegRow {
            mode,
            brightness: quantize::brightness(r.brightness),
            bg,
            m7: RegM7::from(&r.m7),
            bg3_priority: r.bg3_priority,
            tm: quantize::screen_mask(r.tm),
            ts: quantize::screen_mask(r.ts),
            wh0: r.wh0,
            wh1: r.wh1,
            wh2: r.wh2,
            wh3: r.wh3,
            w12sel: r.w12sel,
            w34sel: r.w34sel,
            wobjsel: r.wobjsel,
            wbglog: r.wbglog,
            wobjlog: r.wobjlog,
            tmw: quantize::screen_mask(r.tmw),
            tsw: quantize::screen_mask(r.tsw),
            cgwsel: r.cgwsel,
            cgadsub: r.cgadsub,
            coldata: quantize::coldata15(r.coldata),
        }
    }
}

impl RegRow {
    /// The shared window ranges (WH0-3). Reused by the color window.
    pub fn window_ranges(&self) -> WindowRanges {
        WindowRanges {
            w1_left: self.wh0,
            w1_right: self.wh1,
            w2_left: self.wh2,
            w2_right: self.wh3,
        }
    }

    /// The window-select config for layer `layer` (0..3 = BG1..BG4, 4 = OBJ).
    pub fn layer_window(&self, layer: usize) -> WindowSel {
        // (sel byte, nibble shift, WLOG byte, WLOG field shift) per layer.
        let (sel, shift, log, log_shift) = match layer {
            0 => (self.w12sel, 0, self.wbglog, 0),   // BG1
            1 => (self.w12sel, 4, self.wbglog, 2),   // BG2
            2 => (self.w34sel, 0, self.wbglog, 4),   // BG3
            3 => (self.w34sel, 4, self.wbglog, 6),   // BG4
            _ => (self.wobjsel, 0, self.wobjlog, 0), // OBJ
        };
        WindowSel::from_bits((sel >> shift) & 0x0f, (log >> log_shift) & 0x03)
    }

    /// The COLOR window's select config: WOBJSEL high nibble + WOBJLOG bits 2-3.
    /// Not consumed here — the color-math ticket calls this with `in_window`
    /// to gate its color effect.
    pub fn color_window(&self) -> WindowSel {
        WindowSel::from_bits((self.wobjsel >> 4) & 0x0f, (self.wobjlog >> 2) & 0x03)
    }

    /// CGADSUB bit7: subtract (true) vs add (false).
    pub fn math_subtract(&self) -> bool {
        self.cgadsub & 0x80 != 0
    }
    /// CGADSUB bit6: halve the math result.
    pub fn math_half(&self) -> bool {
        self.cgadsub & 0x40 != 0
    }
    /// CGADSUB bits0-5: is color math enabled for this source layer?
    /// `layer` 0..3 = BG1..BG4, 4 = OBJ, 5 = backdrop.
    pub fn math_layer_enabled(&self, layer: usize) -> bool {
        self.cgadsub & (1 << layer) != 0
    }
    /// CGWSEL bit1: addend is the subscreen (true) vs the COLDATA fixed color (false).
    pub fn add_subscreen(&self) -> bool {
        self.cgwsel & 0x02 != 0
    }
    /// CGWSEL bit0: direct color mode — an 8bpp BG index becomes a BGR555 color
    /// built from its index + tilemap palette bits instead of a CGRAM lookup
    /// (resolved into `RegBg::direct_color` for the rasterizer; see bg.rs).
    pub fn direct_color(&self) -> bool {
        self.cgwsel & 0x01 != 0
    }
    /// CGWSEL bits6-7: clip-to-black region select (0 never,1 outside,2 inside,3 always).
    pub fn clip_mode(&self) -> u8 {
        (self.cgwsel >> 6) & 0x03
    }
    /// CGWSEL bits4-5: prevent-math region select (0 never,1 outside,2 inside,3 always).
    pub fn prevent_mode(&self) -> u8 {
        (self.cgwsel >> 4) & 0x03
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::window::WLog;

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
        assert_eq!(reg.bg[0].offset_map_base, reg.bg[2].map_base);
        assert_eq!(reg.bg[0].offset_screen_size, reg.bg[2].screen_size);
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
    fn obj_oam_high_table_round_trips_x_bit8_and_large() {
        // OAM high table packs 2 bits/sprite: bit0 = X bit 8 (the sign of the 9-bit
        // signed X), bit1 = the `large` size bit. Values below stay in 9-bit signed
        // range (-256..=255) — the only X range OAM can hold.
        let cases: [(i16, bool, u8); 6] = [
            (0, false, 0b00),
            (255, false, 0b00),   // fits in 8 bits, no X bit 8
            (100, true, 0b10),    // large only
            (-1, false, 0b01),    // negative -> X bit 8 set
            (-200, false, 0b01),
            (-256, true, 0b11),   // most-negative X + large
        ];
        for (x, large, bits) in cases {
            let o = Obj { x, large, ..Obj::default() };
            assert_eq!(o.oam_high_bits(), bits, "high bits for x={x} large={large}");
            // Round-trip: the low X byte + the high nibble reconstruct x (9-bit
            // signed) and large exactly.
            let low_x = (x as u16 & 0xff) as u8;
            let r = Obj::from_oam_high(low_x, o.oam_high_bits());
            assert_eq!(r.x, x, "x round-trip for x={x}");
            assert_eq!(r.large, large, "large round-trip for x={x}");
        }
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
        let mut src = LineTableRow::default();
        for (mode, expected) in [
            (0, [2, 2, 2, 2]),
            (2, [4, 4, 0, 0]),
            (3, [8, 4, 0, 0]),
            (4, [8, 2, 0, 0]),
        ] {
            src.mode = mode;
            let reg = RegRow::from(&src);
            assert_eq!(
                [reg.bg[0].bpp, reg.bg[1].bpp, reg.bg[2].bpp, reg.bg[3].bpp],
                expected,
                "mode {mode} bpp"
            );
            assert_eq!(reg.bg[0].mode, reg.mode);
            assert_eq!(reg.bg[3].layer, 3);
        }
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

    #[test]
    fn tm_ts_defaults_and_quantize_on_write() {
        // Playground power-on: main = all five layers, sub = empty.
        let d = LineTableRow::default();
        assert_eq!((d.tm, d.ts), (0x1f, 0x00));
        let reg = RegRow::from(&d);
        assert_eq!((reg.tm, reg.ts), (0x1f, 0x00));
        // 5-bit mask (wraps) on write, like brightness/mode.
        let mut src = LineTableRow::default();
        src.tm = 0x13; // BG1+BG2+OBJ
        src.ts = 0xe4; // high bits set -> masks to 0x04 (BG3 only)
        let reg = RegRow::from(&src);
        assert_eq!(reg.tm, 0x13);
        assert_eq!(reg.ts, 0x04);
    }

    #[test]
    fn window_registers_default_zero_and_round_trip() {
        let d = LineTableRow::default();
        assert_eq!(
            (
                d.wh0, d.wh1, d.wh2, d.wh3, d.w12sel, d.w34sel, d.wobjsel, d.wbglog, d.wobjlog,
                d.tmw, d.tsw
            ),
            (0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)
        );
        let mut src = LineTableRow::default();
        src.wh0 = 64;
        src.wh1 = 192;
        src.wh2 = 10;
        src.wh3 = 250;
        src.w12sel = 0xAB;
        src.w34sel = 0xCD;
        src.wobjsel = 0xEF;
        src.wbglog = 0x1B;
        src.wobjlog = 0x0E;
        src.tmw = 0xE3; // high bits set -> masks to 0x03 (BG1+BG2)
        src.tsw = 0x1f;
        let reg = RegRow::from(&src);
        assert_eq!((reg.wh0, reg.wh1, reg.wh2, reg.wh3), (64, 192, 10, 250));
        assert_eq!((reg.w12sel, reg.w34sel, reg.wobjsel), (0xAB, 0xCD, 0xEF));
        assert_eq!((reg.wbglog, reg.wobjlog), (0x1B, 0x0E));
        assert_eq!(reg.tmw, 0x03); // masked to 5 bits like TM/TS
        assert_eq!(reg.tsw, 0x1f);
    }

    #[test]
    fn regrow_layer_window_and_ranges_decode() {
        let mut src = LineTableRow::default();
        src.wh0 = 1;
        src.wh1 = 2;
        src.wh2 = 3;
        src.wh3 = 4;
        // W12SEL low nibble (BG1) = 0b1011: W1 invert+enable, W2 enable.
        src.w12sel = 0x0B;
        // WBGLOG BG1 field (bits 0-1) = 0b10 = XOR.
        src.wbglog = 0x02;
        let reg = RegRow::from(&src);
        let r = reg.window_ranges();
        assert_eq!((r.w1_left, r.w1_right, r.w2_left, r.w2_right), (1, 2, 3, 4));
        let bg1 = reg.layer_window(0);
        assert!(bg1.w1_enable && bg1.w1_invert && bg1.w2_enable && !bg1.w2_invert);
        assert_eq!(bg1.logic, WLog::Xor);
        // OBJ (layer 4) reads WOBJSEL low nibble + WOBJLOG bits 0-1.
        let mut src2 = LineTableRow::default();
        src2.wobjsel = 0x02; // OBJ W1 enable
        src2.wobjlog = 0x01; // OBJ logic = AND
        let obj = RegRow::from(&src2).layer_window(4);
        assert!(obj.w1_enable && obj.logic == WLog::And);
        // COLOR window: WOBJSEL high nibble + WOBJLOG bits 2-3 (color-math seam).
        let mut src3 = LineTableRow::default();
        src3.wobjsel = 0x20; // COLOR W1 enable (high nibble bit1)
        src3.wobjlog = 0x08; // COLOR logic bits 2-3 = 0b10 = XOR
        let color = RegRow::from(&src3).color_window();
        assert!(color.w1_enable && color.logic == WLog::Xor);
    }

    #[test]
    fn color_math_registers_default_zero_and_round_trip() {
        let d = LineTableRow::default();
        assert_eq!((d.cgwsel, d.cgadsub, d.coldata), (0, 0, 0));
        let mut src = LineTableRow::default();
        src.cgwsel = 0xC2; // clip=always(11), prevent=never(00), addend=subscreen(1), direct=0
        src.cgadsub = 0xC1; // subtract + half + BG1 enable
        src.coldata = 0xFFFF; // masks to 0x7FFF (15-bit)
        let reg = RegRow::from(&src);
        assert_eq!(reg.cgwsel, 0xC2);
        assert_eq!(reg.cgadsub, 0xC1);
        assert_eq!(reg.coldata, 0x7FFF);
    }

    #[test]
    fn cgadsub_decode_accessors() {
        let mut src = LineTableRow::default();
        src.cgadsub = 0x80 | 0x40 | 0b1_1001; // subtract, half, BG1+BG4+OBJ enable
        let reg = RegRow::from(&src);
        assert!(reg.math_subtract());
        assert!(reg.math_half());
        assert!(reg.math_layer_enabled(0)); // BG1
        assert!(!reg.math_layer_enabled(1)); // BG2
        assert!(reg.math_layer_enabled(3)); // BG4
        assert!(reg.math_layer_enabled(4)); // OBJ
        assert!(!reg.math_layer_enabled(5)); // backdrop off
                                             // add + backdrop enable
        src.cgadsub = 0x20;
        let reg = RegRow::from(&src);
        assert!(!reg.math_subtract() && !reg.math_half());
        assert!(reg.math_layer_enabled(5)); // backdrop
    }

    #[test]
    fn cgwsel_decode_accessors() {
        let mut src = LineTableRow::default();
        src.cgwsel = 0b11_10_00_1_0; // clip=11(always), prevent=10(inside), addend=1(subscreen), direct=0
        let reg = RegRow::from(&src);
        assert_eq!(reg.clip_mode(), 0b11);
        assert_eq!(reg.prevent_mode(), 0b10);
        assert!(reg.add_subscreen());
        assert!(!reg.direct_color());
        src.cgwsel = 0x01; // direct color only
        assert!(RegRow::from(&src).direct_color());
    }
}
