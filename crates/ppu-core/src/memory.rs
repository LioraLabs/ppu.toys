//! Clean (NOT byte-accurate) PPU memory model: 15-bit CGRAM, OAM sprites, and
//! "VRAM" as named whole-image graphics sources (decoded RGBA + dimensions).

use std::collections::HashMap;

use crate::registers::Obj;

/// Pack 8-bit-per-channel RGB into a 15-bit SNES BGR555 word
/// (`0bbbbbgggggrrrrr`). This is the canonical CGRAM color format.
pub fn rgb15(r: u8, g: u8, b: u8) -> u16 {
    let r5 = (r >> 3) as u16;
    let g5 = (g >> 3) as u16;
    let b5 = (b >> 3) as u16;
    (b5 << 10) | (g5 << 5) | r5
}

/// Expand a 15-bit BGR555 CGRAM word back to opaque 8-bit RGBA. Inverse of
/// [`rgb15`] up to 5-bit quantization; the rasterizer uses this to sample CGRAM.
pub fn unpack_rgb15(c: u16) -> [u8; 4] {
    let r5 = (c & 0x1f) as u8;
    let g5 = ((c >> 5) & 0x1f) as u8;
    let b5 = ((c >> 10) & 0x1f) as u8;
    let expand = |v: u8| (v << 3) | (v >> 2); // 5-bit -> 8-bit
    [expand(r5), expand(g5), expand(b5), 255]
}

/// A decoded, named whole-image graphics source (a BG layer image or the OBJ
/// sheet). The engine auto-tiles / scrolls / transforms over it; it is never
/// the byte-level VRAM of real hardware.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>, // width * height * 4
}

/// Frame-global PPU memory: palette, sprite table, OBJ sheet selector, and the
/// named image sources referenced by `bg[n].source` / `obj.sheet`.
#[derive(Clone, Debug)]
pub struct Memory {
    /// CGRAM: 256 palette entries, each a 15-bit BGR555 color. Entry 0 is the
    /// backdrop.
    pub cgram: [u16; 256],
    /// OAM: the 128 sprites.
    pub oam: [Obj; 128],
    /// Asset id of the OBJ tile sheet that `obj[i].tile` indexes.
    pub obj_sheet: Option<String>,
    /// Named whole-image sources keyed by upload asset id ("VRAM").
    pub sources: HashMap<String, Source>,
}

impl Default for Memory {
    fn default() -> Self {
        Memory {
            cgram: [0; 256],
            oam: [Obj::default(); 128],
            obj_sheet: None,
            sources: HashMap::new(),
        }
    }
}

impl Memory {
    pub fn new() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb15_packs_extremes() {
        assert_eq!(rgb15(0, 0, 0), 0x0000);
        assert_eq!(rgb15(255, 255, 255), 0x7fff);
        assert_eq!(rgb15(255, 0, 0), 0x001f); // red in low 5 bits
        assert_eq!(rgb15(0, 0, 255), 0x7c00); // blue in high 5 bits
    }

    #[test]
    fn rgb15_roundtrips_through_unpack() {
        for &(r, g, b) in &[(0, 0, 0), (255, 255, 255), (255, 0, 0), (16, 128, 248)] {
            let c = rgb15(r, g, b);
            let px = unpack_rgb15(c);
            // 5-bit quantized re-pack is stable.
            assert_eq!(rgb15(px[0], px[1], px[2]), c);
            assert_eq!(px[3], 255);
        }
    }

    #[test]
    fn memory_defaults_are_empty() {
        let m = Memory::new();
        assert_eq!(m.cgram, [0u16; 256]);
        assert!(m.oam.iter().all(|o| !o.on));
        assert!(m.obj_sheet.is_none());
        assert!(m.sources.is_empty());
    }

    #[test]
    fn sources_are_keyed_by_asset_id() {
        let mut m = Memory::new();
        m.sources.insert(
            "sky".into(),
            Source { width: 2, height: 1, rgba: vec![0; 8] },
        );
        assert_eq!(m.sources.get("sky").unwrap().width, 2);
        assert!(m.sources.get("missing").is_none());
    }
}
