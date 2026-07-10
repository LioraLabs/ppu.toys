//! ppu.toys headless PPU core. Phase-1 register model, byte-accurate memory model
//! (VRAM/CGRAM/OAM), defaults->per-line override resolution, and the Phase-2
//! compositor. The wasm shim drives the Lua VM -> LineTable -> `render_frame`
//! pipeline and exposes it over the TS `PpuCore` seam.

use serde::Serialize;
use std::collections::HashMap;

mod registers;
pub use registers::*;

mod memory;
pub use memory::*;

mod linetable;
pub use linetable::*;

mod bg;
pub use bg::*;

mod mode7;
pub use mode7::*;

mod sprite;
pub use sprite::*;

mod compositor;
pub use compositor::*;

mod lua;
pub use lua::*;

mod quantize;
pub use quantize::*;

mod modes;
pub use modes::*;

mod window;
pub use window::*;

// m4/importer: shared tile-BG importer + reusable quantize/tiles core.
pub mod import;

// m4/importer: Mode 7 importer. Named `import_m7` to avoid a module
// name collision with the shared tile-BG importer. Its local median-cut /
// nearest / dedup overlap import::quantize/import::tiles; unifying them is a
// tracked follow-up (a behavioral change that would re-baseline Mode 7 goldens).
mod import_m7;
pub use import_m7::*;

/// Native SNES PPU output dimensions (the only resolution v1 targets).
pub const WIDTH: usize = 256;
pub const HEIGHT: usize = 224;

/// A single PPU register's mirrored value (the absolute bit pattern), surfaced to
/// the UI inspector.
#[derive(Clone, Debug, Serialize)]
pub struct Register {
    pub addr: u16,
    pub name: String,
    pub value: i32,
    pub changed: bool,
}

/// `setSource` result, matching the TS `{ ok, error? }` shape.
#[derive(Serialize)]
pub struct SetSourceResult {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<LuaErrorView>,
}

/// Serializable Lua compile/runtime error, matching TS `LuaError`.
#[derive(Serialize)]
pub struct LuaErrorView {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
}

impl From<LuaError> for LuaErrorView {
    fn from(e: LuaError) -> Self {
        LuaErrorView {
            message: e.message,
            line: e.line,
            file: e.file,
        }
    }
}

/// A sprite mapped to the JS `OamSprite` shape (camelCase flip fields).
#[derive(Serialize)]
pub struct OamSprite {
    pub x: i32,
    pub y: i32,
    pub tile: u16,
    pub pal: u8,
    pub prio: u8,
    pub large: bool,
    #[serde(rename = "flipX")]
    pub flip_x: bool,
    #[serde(rename = "flipY")]
    pub flip_y: bool,
    pub on: bool,
}

impl From<&Obj> for OamSprite {
    fn from(o: &Obj) -> Self {
        OamSprite {
            x: o.x as i32,
            y: o.y as i32,
            tile: o.tile,
            pal: o.pal,
            prio: o.prio,
            large: o.large,
            flip_x: o.flip_x,
            flip_y: o.flip_y,
            on: o.on,
        }
    }
}

/// Per-frame OBJ overflow diagnostic for the `$213E` STAT77 inspector badges.
/// Read-only status: `range_over`/`time_over` are set if
/// ANY scanline overflowed; `max_sprites`/`max_tiles` are the busiest line's
/// in-range sprite count and attempted tile-sliver count across the frame.
#[derive(Serialize, Default, Clone, Debug, PartialEq, Eq)]
pub struct ObjOverflow {
    #[serde(rename = "rangeOver")]
    pub range_over: bool,
    #[serde(rename = "timeOver")]
    pub time_over: bool,
    #[serde(rename = "maxSprites")]
    pub max_sprites: u16,
    #[serde(rename = "maxTiles")]
    pub max_tiles: u16,
}

/// An uploaded image source, mapped to the JS `AssetInfo` shape.
#[derive(Serialize)]
pub struct AssetInfo {
    pub id: String,
    pub width: u32,
    pub height: u32,
}

/// Derive the inspector register list from the resolved absolute row. Values are
/// the register bit pattern (masked to the register's display width); `changed`
/// is true when `prev` held a different value for that addr.
pub fn derive_registers(row: &RegRow, obsel: &Obsel, prev: &HashMap<u16, i32>) -> Vec<Register> {
    let scroll = |v: i16| (v as u16 & 0x1fff) as i32; // 13-bit display width; ponytail: real BG H/VOFS are 10-bit, uniform 13-bit mask is a v1 inspector simplification
    let m7 = |v: i16| (v as u16) as i32; // raw Q8 bit pattern (16-bit)
                                         // BGMODE: mode bits 0-2 | BG3-priority bit 3 | BG1..BG4 16x16-tile flags in bits 4-7.
    let bgmode = row.mode as i32
        | ((row.bg3_priority as i32) << 3)
        | (row.bg.iter().enumerate())
            .map(|(i, b)| ((b.tile_size == 16) as i32) << (4 + i))
            .sum::<i32>();
    // BGnSC: screen size bits 0-1 | tilemap base field bits 2-7.
    let sc = |b: &RegBg| (b.screen_size as i32) | (((b.map_base >> 10) as i32) << 2);
    // BGnNBA: two 4-bit char base fields per register.
    let nba = |lo: &RegBg, hi: &RegBg| {
        ((lo.char_base >> 12) as i32) | (((hi.char_base >> 12) as i32) << 4)
    };
    let m7sel =
        (row.m7.flip_x as i32) | ((row.m7.flip_y as i32) << 1) | ((row.m7.repeat as i32) << 6);
    // OBSEL ($2101): char-base name field bits 0-2 (char_base >> 13) | name-select
    // bits 3-4 | size-select bits 5-7.
    let obsel_val = ((obsel.char_base >> 13) as i32)
        | ((obsel.name_select as i32) << 3)
        | ((obsel.size_sel as i32) << 5);
    // MOSAIC ($2106): size bits 0-3 | per-BG enable bits 4-7 (BG1..BG4).
    let mosaic = (row.mosaic_size as i32 & 0x0f)
        | row
            .mosaic_enable
            .iter()
            .enumerate()
            .map(|(i, &e)| (e as i32) << (4 + i))
            .sum::<i32>();
    let entries: [(u16, &str, i32); 40] = [
        (
            0x2100,
            "INIDISP",
            row.brightness as i32 | ((row.force_blank as i32) << 7),
        ),
        (0x2105, "BGMODE", bgmode),
        (0x2106, "MOSAIC", mosaic),
        (0x2101, "OBSEL", obsel_val),
        (0x2107, "BG1SC", sc(&row.bg[0])),
        (0x2108, "BG2SC", sc(&row.bg[1])),
        (0x2109, "BG3SC", sc(&row.bg[2])),
        (0x210a, "BG4SC", sc(&row.bg[3])),
        (0x210b, "BG12NBA", nba(&row.bg[0], &row.bg[1])),
        (0x210c, "BG34NBA", nba(&row.bg[2], &row.bg[3])),
        (0x210d, "BG1HOFS", scroll(row.bg[0].scroll_x)),
        (0x210e, "BG1VOFS", scroll(row.bg[0].scroll_y)),
        (0x210f, "BG2HOFS", scroll(row.bg[1].scroll_x)),
        (0x2110, "BG2VOFS", scroll(row.bg[1].scroll_y)),
        (0x2111, "BG3HOFS", scroll(row.bg[2].scroll_x)),
        (0x2112, "BG3VOFS", scroll(row.bg[2].scroll_y)),
        (0x2113, "BG4HOFS", scroll(row.bg[3].scroll_x)),
        (0x2114, "BG4VOFS", scroll(row.bg[3].scroll_y)),
        (0x211a, "M7SEL", m7sel),
        (0x211b, "M7A", m7(row.m7.a)),
        (0x211c, "M7B", m7(row.m7.b)),
        (0x211d, "M7C", m7(row.m7.c)),
        (0x211e, "M7D", m7(row.m7.d)),
        (0x2123, "W12SEL", row.w12sel as i32),
        (0x2124, "W34SEL", row.w34sel as i32),
        (0x2125, "WOBJSEL", row.wobjsel as i32),
        (0x2126, "WH0", row.wh0 as i32),
        (0x2127, "WH1", row.wh1 as i32),
        (0x2128, "WH2", row.wh2 as i32),
        (0x2129, "WH3", row.wh3 as i32),
        (0x212a, "WBGLOG", row.wbglog as i32),
        (0x212b, "WOBJLOG", row.wobjlog as i32),
        (0x212c, "TM", row.tm as i32),
        (0x212d, "TS", row.ts as i32),
        (0x212e, "TMW", row.tmw as i32),
        (0x212f, "TSW", row.tsw as i32),
        (0x2130, "CGWSEL", row.cgwsel as i32),
        (0x2131, "CGADSUB", row.cgadsub as i32),
        (0x2132, "COLDATA", row.coldata as i32),
        (0x2133, "SETINI", row.setini as i32),
    ];
    entries
        .iter()
        .map(|&(addr, name, value)| Register {
            addr,
            name: name.to_string(),
            value,
            changed: prev.get(&addr).is_some_and(|&p| p != value),
        })
        .collect()
}

#[cfg(target_arch = "wasm32")]
mod wasm;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registers::{LineTableRow, Obsel, RegRow};
    use std::collections::HashMap;

    #[test]
    fn derive_registers_reports_inidisp_and_bgmode() {
        let mut ltr = LineTableRow::default();
        ltr.brightness = 7;
        ltr.mode = 3;
        let row = RegRow::from(&ltr);
        let regs = derive_registers(&row, &Obsel::default(), &HashMap::new());
        let inidisp = regs.iter().find(|r| r.name == "INIDISP").unwrap();
        assert_eq!(inidisp.addr, 0x2100);
        assert_eq!(inidisp.value, 7);
        let bgmode = regs.iter().find(|r| r.name == "BGMODE").unwrap();
        assert_eq!(bgmode.value, 3);
    }

    #[test]
    fn derive_registers_reports_setini_extbg() {
        let mut ltr = LineTableRow::default();
        ltr.setini = 0x40; // EXTBG on
        let row = RegRow::from(&ltr);
        let regs = derive_registers(&row, &Obsel::default(), &HashMap::new());
        let setini = regs.iter().find(|r| r.name == "SETINI").unwrap();
        assert_eq!(setini.addr, 0x2133);
        assert_eq!(setini.value, 0x40);
    }

    #[test]
    fn derive_registers_inidisp_includes_force_blank_bit7() {
        let mut ltr = LineTableRow::default();
        ltr.brightness = 7;
        ltr.force_blank = true;
        let row = RegRow::from(&ltr);
        let regs = derive_registers(&row, &Obsel::default(), &HashMap::new());
        let inidisp = regs.iter().find(|r| r.name == "INIDISP").unwrap();
        assert_eq!(inidisp.value, 0x87); // brightness 7 | force-blank bit 7
    }

    #[test]
    fn derive_registers_changed_flag_tracks_prev() {
        let row = RegRow::from(&LineTableRow::default()); // brightness 15, mode 1
        let first = derive_registers(&row, &Obsel::default(), &HashMap::new());
        assert!(first.iter().all(|r| !r.changed));
        let mut prev = HashMap::new();
        prev.insert(0x2100u16, 7i32);
        prev.insert(0x2105u16, 1i32);
        let next = derive_registers(&row, &Obsel::default(), &prev);
        assert!(next.iter().find(|r| r.addr == 0x2100).unwrap().changed);
        assert!(!next.iter().find(|r| r.addr == 0x2105).unwrap().changed);
    }

    #[test]
    fn derive_registers_shows_absolute_scroll() {
        let mut row = RegRow::from(&LineTableRow::default());
        row.bg[0].scroll_x = 419; // absolute, already quantized
        let regs = derive_registers(&row, &Obsel::default(), &HashMap::new());
        let bg1hofs = regs.iter().find(|r| r.name == "BG1HOFS").unwrap();
        assert_eq!(bg1hofs.value, 419); // truthful, matches what renders
    }

    #[test]
    fn derive_registers_masks_negative_scroll_to_13_bit() {
        let mut row = RegRow::from(&LineTableRow::default());
        row.bg[0].scroll_x = -256; // absolute i16
        let regs = derive_registers(&row, &Obsel::default(), &HashMap::new());
        let bg1hofs = regs.iter().find(|r| r.name == "BG1HOFS").unwrap();
        // (-256i16 as u16) & 0x1fff = 0xFF00 & 0x1FFF = 0x1F00 = 7936
        assert_eq!(bg1hofs.value, 7936);
    }

    #[test]
    fn bgmode_packs_tile_size_bits() {
        let mut ltr = LineTableRow::default(); // mode 1
        ltr.bg[0].tile_size = 16; // BGMODE bit 4
        ltr.bg[3].tile_size = 16; // BGMODE bit 7
        let row = RegRow::from(&ltr);
        let regs = derive_registers(&row, &Obsel::default(), &HashMap::new());
        let bgmode = regs.iter().find(|r| r.name == "BGMODE").unwrap();
        assert_eq!(bgmode.value, 0x91); // 1 | 1<<4 | 1<<7
    }

    #[test]
    fn bgnsc_packs_screen_size_and_map_base() {
        let mut ltr = LineTableRow::default();
        ltr.bg[0].screen_size = 3;
        ltr.bg[0].map_base = 0x0800; // field 2 -> bits 2-7 = 2
        let row = RegRow::from(&ltr);
        let regs = derive_registers(&row, &Obsel::default(), &HashMap::new());
        let bg1sc = regs.iter().find(|r| r.name == "BG1SC").unwrap();
        assert_eq!(bg1sc.addr, 0x2107);
        assert_eq!(bg1sc.value, 0x0b); // 3 | 2<<2
        let bg4sc = regs.iter().find(|r| r.name == "BG4SC").unwrap();
        assert_eq!(bg4sc.addr, 0x210a);
        assert_eq!(bg4sc.value, 0);
    }

    #[test]
    fn nba_packs_two_char_bases_per_register() {
        let mut ltr = LineTableRow::default();
        ltr.bg[0].char_base = 0x1000; // field 1 -> low nibble of BG12NBA
        ltr.bg[1].char_base = 0x2000; // field 2 -> high nibble of BG12NBA
        ltr.bg[2].char_base = 0x3000; // field 3 -> low nibble of BG34NBA
        let row = RegRow::from(&ltr);
        let regs = derive_registers(&row, &Obsel::default(), &HashMap::new());
        let nba12 = regs.iter().find(|r| r.name == "BG12NBA").unwrap();
        assert_eq!(nba12.addr, 0x210b);
        assert_eq!(nba12.value, 0x21);
        let nba34 = regs.iter().find(|r| r.name == "BG34NBA").unwrap();
        assert_eq!(nba34.addr, 0x210c);
        assert_eq!(nba34.value, 0x03);
    }

    #[test]
    fn m7sel_packs_flip_and_repeat() {
        let mut ltr = LineTableRow::default();
        ltr.m7.flip_x = true;
        ltr.m7.repeat = 3;
        let row = RegRow::from(&ltr);
        let regs = derive_registers(&row, &Obsel::default(), &HashMap::new());
        let m7sel = regs.iter().find(|r| r.name == "M7SEL").unwrap();
        assert_eq!(m7sel.addr, 0x211a);
        assert_eq!(m7sel.value, 0xc1); // bit0 flip_x | bits6-7 repeat=3
    }

    #[test]
    fn oam_sprite_serializes_camelcase() {
        let obj = Obj {
            on: true,
            flip_x: true,
            flip_y: false,
            tile: 5,
            ..Obj::default()
        };
        let s = OamSprite::from(&obj);
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!(json["flipX"], true);
        assert_eq!(json["flipY"], false);
        assert_eq!(json["on"], true);
        assert_eq!(json["tile"], 5);
        assert_eq!(json["large"], false);
        assert!(json.get("flip_x").is_none());
    }

    #[test]
    fn obj_overflow_serializes_camelcase() {
        let ov = ObjOverflow {
            range_over: true,
            time_over: false,
            max_sprites: 40,
            max_tiles: 34,
        };
        let json = serde_json::to_value(&ov).unwrap();
        assert_eq!(json["rangeOver"], true);
        assert_eq!(json["timeOver"], false);
        assert_eq!(json["maxSprites"], 40);
        assert_eq!(json["maxTiles"], 34);
        assert!(json.get("range_over").is_none());
    }

    #[test]
    fn derive_registers_includes_m6_screen_window_and_math() {
        let mut ltr = LineTableRow::default();
        ltr.tm = 0x13;
        ltr.ts = 0x04;
        ltr.tmw = 0x01;
        ltr.tsw = 0x02;
        ltr.wh0 = 32;
        ltr.wh1 = 200;
        ltr.wh2 = 10;
        ltr.wh3 = 240;
        ltr.w12sel = 0x0b;
        ltr.w34sel = 0xcd;
        ltr.wobjsel = 0x2e;
        ltr.wbglog = 0x1b;
        ltr.wobjlog = 0x0e;
        ltr.cgwsel = 0x42;
        ltr.cgadsub = 0x81;
        ltr.coldata = 0x7c1f;
        let row = RegRow::from(&ltr);
        let regs = derive_registers(&row, &Obsel::default(), &HashMap::new());
        let val = |name: &str| regs.iter().find(|r| r.name == name).unwrap().value;
        assert_eq!(val("TM"), 0x13);
        assert_eq!(val("TS"), 0x04);
        assert_eq!(val("TMW"), 0x01);
        assert_eq!(val("TSW"), 0x02);
        assert_eq!(val("WH0"), 32);
        assert_eq!(val("WH1"), 200);
        assert_eq!(val("WH2"), 10);
        assert_eq!(val("WH3"), 240);
        assert_eq!(val("W12SEL"), 0x0b);
        assert_eq!(val("W34SEL"), 0xcd);
        assert_eq!(val("WOBJSEL"), 0x2e);
        assert_eq!(val("WBGLOG"), 0x1b);
        assert_eq!(val("WOBJLOG"), 0x0e);
        assert_eq!(val("CGWSEL"), 0x42);
        assert_eq!(val("CGADSUB"), 0x81);
        assert_eq!(val("COLDATA"), 0x7c1f);
        let addr = |name: &str| regs.iter().find(|r| r.name == name).unwrap().addr;
        assert_eq!(addr("TM"), 0x212c);
        assert_eq!(addr("CGADSUB"), 0x2131);
        assert_eq!(addr("COLDATA"), 0x2132);
    }

    #[test]
    fn derive_registers_includes_obsel() {
        let row = RegRow::from(&LineTableRow::default());
        let obsel = Obsel {
            char_base: 0x2000, // >>13 = 1 in bits 0-2
            name_select: 2,    // bits 3-4 = 0b10
            size_sel: 5,       // bits 5-7 = 0b101
        };
        let regs = derive_registers(&row, &obsel, &HashMap::new());
        let o = regs.iter().find(|r| r.name == "OBSEL").unwrap();
        assert_eq!(o.addr, 0x2101);
        // 1 | (2<<3) | (5<<5) = 1 | 16 | 160 = 177
        assert_eq!(o.value, 0xB1);
    }

    #[test]
    fn derive_registers_includes_mosaic_2106() {
        let mut ltr = LineTableRow::default();
        ltr.mosaic_size = 5;
        ltr.mosaic_enable = [true, false, true, false]; // BG1 + BG3 -> bits 4 and 6
        let row = RegRow::from(&ltr);
        let regs = derive_registers(&row, &Obsel::default(), &HashMap::new());
        let mosaic = regs.iter().find(|r| r.name == "MOSAIC").unwrap();
        assert_eq!(mosaic.addr, 0x2106);
        // size 5 (bits 0-3) | BG1 (bit4) | BG3 (bit6) = 0x05 | 0x10 | 0x40 = 0x55.
        assert_eq!(mosaic.value, 0x55);
    }
}
