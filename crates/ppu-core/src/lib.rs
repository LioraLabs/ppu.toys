//! ppu.toys headless PPU core. Phase-1 register model, clean memory model
//! (CGRAM/OAM/named image sources), and defaults->per-line override resolution.
//! Rasterization is still a placeholder; the wasm shim uses the `placeholder_*`
//! helpers until the real compositor lands.

use serde::Serialize;

mod registers;
pub use registers::*;

mod memory;
pub use memory::*;

mod linetable;
pub use linetable::*;

mod mode7;
pub use mode7::*;

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

/// Deterministic placeholder framebuffer: an x/y color ramp that animates
/// slightly with time/frame so the UI shows live motion. Replaced by the real
/// rasterizer in M1.
pub fn placeholder_framebuffer(t: f64, f: u32) -> Vec<u8> {
    let b = (((t * 60.0) as i64).rem_euclid(256) as u8) ^ (f as u8);
    let mut fb = vec![0u8; WIDTH * HEIGHT * 4];
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let i = (y * WIDTH + x) * 4;
            fb[i] = x as u8;
            fb[i + 1] = y as u8;
            fb[i + 2] = b;
            fb[i + 3] = 255;
        }
    }
    fb
}

/// A couple of fake registers so the inspector has something to render in M0.
pub fn placeholder_registers() -> Vec<Register> {
    vec![
        Register { addr: 0x2100, name: "INIDISP".into(), value: 0x0f, changed: false },
        Register { addr: 0x2105, name: "BGMODE".into(), value: 0x01, changed: false },
    ]
}

/// A 256-entry CGRAM gradient (15-bit packed) for the palette grid.
pub fn placeholder_cgram() -> Vec<u16> {
    (0..256).map(|i| ((i as u16) * 0x84) & 0x7fff).collect()
}

/// Result of `setSource`, matching the TS `{ ok, error? }` shape.
#[derive(Serialize)]
pub struct SetSourceResult {
    pub ok: bool,
}

#[cfg(target_arch = "wasm32")]
mod wasm;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_framebuffer_is_full_size_and_opaque() {
        let fb = placeholder_framebuffer(0.0, 0);
        assert_eq!(fb.len(), WIDTH * HEIGHT * 4);
        assert!(fb.chunks(4).all(|px| px[3] == 255));
    }

    #[test]
    fn placeholder_registers_and_cgram_have_expected_shape() {
        let regs = placeholder_registers();
        assert!(regs.iter().any(|r| r.name == "BGMODE"));
        assert_eq!(placeholder_cgram().len(), 256);
    }
}
