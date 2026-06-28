//! ppu.toys headless PPU core. Phase-1 register model, clean memory model
//! (CGRAM/OAM/named image sources), defaults->per-line override resolution, and
//! the Phase-2 compositor. The wasm shim drives the Lua VM -> LineTable ->
//! `render_frame` pipeline and exposes it over the TS `PpuCore` seam.

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

/// Native SNES PPU output dimensions (the only resolution v1 targets).
pub const WIDTH: usize = 256;
pub const HEIGHT: usize = 224;

/// A single PPU register's mirrored value, surfaced to the UI inspector.
#[derive(Clone, Debug, Serialize)]
pub struct Register {
    pub addr: u16,
    pub name: String,
    pub value: u8,
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
}

/// A sprite mapped to the JS `OamSprite` shape (camelCase flip fields).
#[derive(Serialize)]
pub struct OamSprite {
    pub x: i32,
    pub y: i32,
    pub tile: u16,
    pub pal: u8,
    pub prio: u8,
    pub size: u8,
    #[serde(rename = "flipX")]
    pub flip_x: bool,
    #[serde(rename = "flipY")]
    pub flip_y: bool,
    pub on: bool,
}

impl From<&Obj> for OamSprite {
    fn from(o: &Obj) -> Self {
        OamSprite {
            x: o.x.round() as i32,
            y: o.y.round() as i32,
            tile: o.tile,
            pal: o.pal,
            prio: o.prio,
            size: o.size,
            flip_x: o.flip_x,
            flip_y: o.flip_y,
            on: o.on,
        }
    }
}

/// An uploaded image source, mapped to the JS `AssetInfo` shape.
#[derive(Serialize)]
pub struct AssetInfo {
    pub id: String,
    pub width: u32,
    pub height: u32,
}

/// Derive the inspector register list from the resolved frame-wide row. `changed`
/// is true when `prev` held a different value for that addr (false on first frame).
pub fn derive_registers(row: &LineTableRow, prev: &HashMap<u16, u8>) -> Vec<Register> {
    let scroll = |v: f32| (v as i64).rem_euclid(256) as u8;
    let m7 = |v: f32| ((v * 256.0) as i64).rem_euclid(256) as u8;
    let entries: [(u16, &str, u8); 14] = [
        (0x2100, "INIDISP", row.brightness),
        (0x2105, "BGMODE", row.mode),
        (0x210d, "BG1HOFS", scroll(row.bg[0].scroll_x)),
        (0x210e, "BG1VOFS", scroll(row.bg[0].scroll_y)),
        (0x210f, "BG2HOFS", scroll(row.bg[1].scroll_x)),
        (0x2110, "BG2VOFS", scroll(row.bg[1].scroll_y)),
        (0x2111, "BG3HOFS", scroll(row.bg[2].scroll_x)),
        (0x2112, "BG3VOFS", scroll(row.bg[2].scroll_y)),
        (0x2113, "BG4HOFS", scroll(row.bg[3].scroll_x)),
        (0x2114, "BG4VOFS", scroll(row.bg[3].scroll_y)),
        (0x211b, "M7A", m7(row.m7.a)),
        (0x211c, "M7B", m7(row.m7.b)),
        (0x211d, "M7C", m7(row.m7.c)),
        (0x211e, "M7D", m7(row.m7.d)),
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
    use crate::registers::LineTableRow;
    use std::collections::HashMap;

    #[test]
    fn derive_registers_reports_inidisp_and_bgmode() {
        let mut row = LineTableRow::default();
        row.brightness = 7;
        row.mode = 3;
        let regs = derive_registers(&row, &HashMap::new());
        let inidisp = regs.iter().find(|r| r.name == "INIDISP").unwrap();
        assert_eq!(inidisp.addr, 0x2100);
        assert_eq!(inidisp.value, 7);
        let bgmode = regs.iter().find(|r| r.name == "BGMODE").unwrap();
        assert_eq!(bgmode.value, 3);
    }

    #[test]
    fn derive_registers_changed_flag_tracks_prev() {
        let row = LineTableRow::default(); // brightness 15
        let first = derive_registers(&row, &HashMap::new());
        assert!(first.iter().all(|r| !r.changed));
        let mut prev = HashMap::new();
        prev.insert(0x2100u16, 7u8);
        prev.insert(0x2105u16, 1u8);
        let next = derive_registers(&row, &prev);
        assert!(next.iter().find(|r| r.addr == 0x2100).unwrap().changed);
        assert!(!next.iter().find(|r| r.addr == 0x2105).unwrap().changed);
    }

    #[test]
    fn oam_sprite_serializes_camelcase() {
        let obj = Obj { on: true, flip_x: true, flip_y: false, tile: 5, ..Obj::default() };
        let s = OamSprite::from(&obj);
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!(json["flipX"], true);
        assert_eq!(json["flipY"], false);
        assert_eq!(json["on"], true);
        assert_eq!(json["tile"], 5);
        assert!(json.get("flip_x").is_none());
    }
}
