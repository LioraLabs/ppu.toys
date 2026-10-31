//! The console around the CPU: cartridge mapping, WRAM, MMIO, DMA/HDMA, timing, the PPU bridge
//! into ppu-core, and the frame loop.
use crate::{Apu, Cpu};

pub struct Snes {
    pub cpu: Cpu,
    pub apu: Apu,
}

impl Snes {
    /// Load a headerless `.sfc` (LoROM or HiROM, detected from the internal header).
    pub fn new(_rom: &[u8]) -> Result<Snes, String> {
        Err("ppu-cart: not built yet".into())
    }
    /// Run until the PPU finishes the next frame (to the start of the following VBlank).
    pub fn run_frame(&mut self) {}
    /// Held buttons for controller 0 or 1 (`crate::pad` bits), latched by the next auto-read.
    pub fn set_pad(&mut self, _port: usize, _buttons: u16) {}
    /// The last frame, 256x224 (or 239) RGBA8.
    pub fn framebuffer(&self) -> &[u8] {
        &[]
    }
    /// Work RAM ($7E:0000-$7F:FFFF), for tools that read game state by symbol.
    pub fn wram(&self) -> &[u8] {
        &[]
    }
    /// Master clocks spent at each 24-bit PC since the last call, for native profiling; cleared on read.
    pub fn take_pc_ticks(&mut self) -> Vec<(u32, u64)> {
        Vec::new()
    }
}
