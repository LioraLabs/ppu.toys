//! Pinned-override register set (M9). The UI pins a register value; pins are
//! applied as a FINAL LineTable hook after `frame()` + all script hdma hooks
//! (lua.rs), so a pinned value wins over every script write on every scanline.
//! Decoding happens at the authoring `LineTableRow` level; quantization and
//! derived fields (bpp / mosaic / direct_color) re-resolve at build() as usual.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::registers::LineTableRow;

/// One pinned register, in the TS `PinnedRegister` shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct PinnedRegister {
    pub addr: u16,
    pub value: i32,
}

/// The pinned-override set: addr -> value bit pattern (same encoding as
/// `derive_registers` displays). BTreeMap keeps `list()` in stable addr order.
#[derive(Clone, Debug, Default)]
pub struct Pins {
    map: BTreeMap<u16, i32>,
}

impl Pins {
    pub fn pin(&mut self, addr: u16, value: i32) {
        self.map.insert(addr, value);
    }
    pub fn unpin(&mut self, addr: u16) {
        self.map.remove(&addr);
    }
    pub fn clear(&mut self) {
        self.map.clear();
    }
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
    pub fn list(&self) -> Vec<PinnedRegister> {
        self.map
            .iter()
            .map(|(&addr, &value)| PinnedRegister { addr, value })
            .collect()
    }
    /// Decode every pinned register into `row`. Unknown / unsupported addrs
    /// (e.g. OBSEL $2101, which is frame-global Memory, not per-line state)
    /// are stored + listed but no-op here.
    pub fn apply(&self, row: &mut LineTableRow) {
        for (&addr, &v) in &self.map {
            apply_one(addr, v, row);
        }
    }
}

/// Decode one register value into the authoring row. Inverse of the encodings
/// in `derive_registers` (lib.rs) — the round-trip test below pins the two
/// together.
fn apply_one(addr: u16, v: i32, row: &mut LineTableRow) {
    match addr {
        0x2100 => {
            row.brightness = (v & 0x0f) as u8;
            row.force_blank = v & 0x80 != 0;
        }
        0x2105 => {
            row.mode = (v & 0x07) as u8;
            row.bg3_priority = v & 0x08 != 0;
            for i in 0..4 {
                row.bg[i].tile_size = if v & (1 << (4 + i)) != 0 { 16 } else { 8 };
            }
        }
        0x2106 => {
            row.mosaic_size = (v & 0x0f) as u8;
            for i in 0..4 {
                row.mosaic_enable[i] = v & (1 << (4 + i)) != 0;
            }
        }
        0x2107..=0x210a => {
            let bg = &mut row.bg[(addr - 0x2107) as usize];
            bg.screen_size = (v & 0x03) as u8;
            bg.map_base = (((v >> 2) & 0x3f) as u32) << 10;
        }
        0x210b => {
            row.bg[0].char_base = ((v & 0x0f) as u32) << 12;
            row.bg[1].char_base = (((v >> 4) & 0x0f) as u32) << 12;
        }
        0x210c => {
            row.bg[2].char_base = ((v & 0x0f) as u32) << 12;
            row.bg[3].char_base = (((v >> 4) & 0x0f) as u32) << 12;
        }
        0x210d..=0x2114 => {
            // 13-bit display value (see derive_registers' `scroll`), sign-extended.
            let s = ((((v as u16) & 0x1fff) << 3) as i16 >> 3) as f32;
            let i = ((addr - 0x210d) / 2) as usize;
            if (addr - 0x210d) % 2 == 0 {
                row.bg[i].scroll_x = s;
            } else {
                row.bg[i].scroll_y = s;
            }
        }
        0x211a => {
            row.m7.flip_x = v & 0x01 != 0;
            row.m7.flip_y = v & 0x02 != 0;
            row.m7.repeat = ((v >> 6) & 0x03) as u8;
        }
        0x211b..=0x211e => {
            // Raw Q8 bit pattern -> authoring float (quantize::m7_matrix inverts).
            let q = (v as u16) as i16 as f32 / 256.0;
            match addr {
                0x211b => row.m7.a = q,
                0x211c => row.m7.b = q,
                0x211d => row.m7.c = q,
                _ => row.m7.d = q,
            }
        }
        0x2123 => row.w12sel = v as u8,
        0x2124 => row.w34sel = v as u8,
        0x2125 => row.wobjsel = v as u8,
        0x2126 => row.wh0 = v as u8,
        0x2127 => row.wh1 = v as u8,
        0x2128 => row.wh2 = v as u8,
        0x2129 => row.wh3 = v as u8,
        0x212a => row.wbglog = v as u8,
        0x212b => row.wobjlog = v as u8,
        0x212c => row.tm = v as u8,
        0x212d => row.ts = v as u8,
        0x212e => row.tmw = v as u8,
        0x212f => row.tsw = v as u8,
        0x2130 => row.cgwsel = v as u8,
        0x2131 => row.cgadsub = v as u8,
        0x2132 => row.coldata = (v as u16) & 0x7fff,
        0x2133 => row.setini = v as u8,
        _ => {} // OBSEL ($2101) + unknown addrs: no-op (frame-global / unsupported)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::derive_registers;
    use crate::registers::{LineTableRow, Obsel, RegRow};
    use std::collections::HashMap;

    #[test]
    fn pin_unpin_clear_and_stable_list_order() {
        let mut p = Pins::default();
        assert!(p.is_empty());
        p.pin(0x2130, 2);
        p.pin(0x2100, 15);
        p.pin(0x2100, 7); // re-pin overwrites
        assert_eq!(
            p.list(),
            vec![
                PinnedRegister { addr: 0x2100, value: 7 },
                PinnedRegister { addr: 0x2130, value: 2 },
            ]
        );
        p.unpin(0x2100);
        assert_eq!(p.list().len(), 1);
        p.clear();
        assert!(p.is_empty());
    }

    #[test]
    fn apply_decodes_inidisp_bgmode_and_mosaic() {
        let mut p = Pins::default();
        p.pin(0x2100, 0x87); // brightness 7 + force blank
        p.pin(0x2105, 0x99); // mode 1, BG3-prio, BG1+BG4 16px tiles
        p.pin(0x2106, 0x55); // size 5, BG1+BG3 enable
        let mut row = LineTableRow::default();
        p.apply(&mut row);
        assert_eq!(row.brightness, 7);
        assert!(row.force_blank);
        assert_eq!(row.mode, 1);
        assert!(row.bg3_priority);
        assert_eq!([row.bg[0].tile_size, row.bg[1].tile_size, row.bg[3].tile_size], [16, 8, 16]);
        assert_eq!(row.mosaic_size, 5);
        assert_eq!(row.mosaic_enable, [true, false, true, false]);
    }

    #[test]
    fn apply_sign_extends_13_bit_scroll() {
        let mut p = Pins::default();
        p.pin(0x210d, 7936); // 0x1F00 = -256 in 13-bit
        p.pin(0x2110, 419); // BG2VOFS positive
        let mut row = LineTableRow::default();
        p.apply(&mut row);
        assert_eq!(row.bg[0].scroll_x, -256.0);
        assert_eq!(row.bg[1].scroll_y, 419.0);
    }

    #[test]
    fn apply_decodes_m7_q8_matrix_and_m7sel() {
        let mut p = Pins::default();
        p.pin(0x211b, 128); // 0.5 in Q8
        p.pin(0x211e, 0xff00); // -1.0 in Q8 (bit pattern)
        p.pin(0x211a, 0xc1); // flip_x + repeat 3
        let mut row = LineTableRow::default();
        p.apply(&mut row);
        assert_eq!(row.m7.a, 0.5);
        assert_eq!(row.m7.d, -1.0);
        assert!(row.m7.flip_x && !row.m7.flip_y);
        assert_eq!(row.m7.repeat, 3);
    }

    #[test]
    fn unknown_addr_is_a_no_op() {
        let mut p = Pins::default();
        p.pin(0x2101, 0xff); // OBSEL: frame-global, unsupported
        p.pin(0x9999, 1);
        let mut row = LineTableRow::default();
        p.apply(&mut row);
        assert_eq!(RegRow::from(&row), RegRow::from(&LineTableRow::default()));
        assert_eq!(p.list().len(), 2); // still stored + listed
    }

    /// The contract test: pinning every value derive_registers reports, then
    /// re-deriving, reproduces the same values (apply is derive's inverse).
    #[test]
    fn apply_round_trips_through_derive_registers() {
        let mut src = LineTableRow::default();
        src.brightness = 7;
        src.mode = 1;
        src.bg3_priority = true;
        src.bg[0].scroll_x = -100.0;
        src.bg[1].scroll_y = 300.0;
        src.bg[0].tile_size = 16;
        src.bg[0].map_base = 0x0800;
        src.bg[2].char_base = 0x3000;
        src.mosaic_size = 3;
        src.mosaic_enable = [true, false, false, true];
        src.m7.a = 0.5;
        src.m7.repeat = 2;
        src.tm = 0x13;
        src.ts = 0x04;
        src.wh0 = 32;
        src.wh1 = 200;
        src.w12sel = 0x0b;
        src.cgwsel = 0x42;
        src.cgadsub = 0x81;
        src.coldata = 0x7c1f;
        src.setini = 0x40;
        let want = derive_registers(&RegRow::from(&src), &Obsel::default(), &HashMap::new());

        let mut p = Pins::default();
        for r in &want {
            if r.addr == 0x2101 {
                continue; // OBSEL: frame-global, not pinnable
            }
            p.pin(r.addr, r.value);
        }
        let mut row = LineTableRow::default();
        p.apply(&mut row);
        let got = derive_registers(&RegRow::from(&row), &Obsel::default(), &HashMap::new());
        for (w, g) in want.iter().zip(&got) {
            if w.addr == 0x2101 {
                continue;
            }
            assert_eq!((g.addr, g.value), (w.addr, w.value), "{} round-trip", w.name);
        }
    }
}
