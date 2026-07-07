//! Byte-accurate PPU memory: word-addressed VRAM, 15-bit CGRAM, and OAM
//! sprites. VRAM/CGRAM/OAM are memory — reads return stored values — while
//! the PPU registers stay write-only latches (registers.rs / quantize.rs).

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

/// Frame-global PPU memory: word-addressed VRAM, CGRAM palette, OAM sprite
/// table, and the OBJ sheet selector referenced by `obj.sheet`.
#[derive(Clone, Debug)]
pub struct Memory {
    /// VRAM: 64KB as 32K 16-bit words, word-addressed like hardware
    /// ($0000-$7FFF). Holds tile char data AND tilemaps, bound by the
    /// BGnSC/BGnNBA binding registers. Mode 7 uses the byte-interleaved layout
    /// at word 0 (low byte = tilemap, high byte = char; m4/mode7).
    pub vram: [u16; 0x8000],
    /// CGRAM: 256 palette entries, each a 15-bit BGR555 color. Entry 0 is the
    /// backdrop.
    pub cgram: [u16; 256],
    /// OAM: the 128 sprites.
    pub oam: [Obj; 128],
    /// Asset id of the OBJ tile sheet that `obj[i].tile` indexes.
    pub obj_sheet: Option<String>,
}

impl Default for Memory {
    fn default() -> Self {
        Memory {
            vram: [0; 0x8000],
            cgram: [0; 256],
            oam: [Obj::default(); 128],
            obj_sheet: None,
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
    }

    #[test]
    fn vram_is_32k_words_zeroed() {
        let m = Memory::new();
        assert_eq!(m.vram.len(), 0x8000); // 64KB, word-addressed
        assert!(m.vram.iter().all(|&w| w == 0));
    }

    #[test]
    fn vram_reads_return_stored_words() {
        let mut m = Memory::new();
        m.vram[0x0000] = 0xbeef;
        m.vram[0x7fff] = 0x1234;
        assert_eq!(m.vram[0x0000], 0xbeef); // memory, not a write-only latch
        assert_eq!(m.vram[0x7fff], 0x1234);
    }
}
