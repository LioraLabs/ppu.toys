//! A SNES console that runs cartridge ROMs (`.sfc`), written from hardware documentation only.
//! The PPU and S-DSP are ppu-core's; this crate adds the 65816 CPU, the SPC700 audio CPU, the
//! memory map, DMA/HDMA, the CPU-side I/O registers and the timing that ties them together.
//!
//! Module ownership (each is built and tested on its own against the interfaces below):
//! - `cpu`: the 65816 (`Cpu`), driven through `Bus`.
//! - `apu`: the SPC700, its 64 KB RAM, timers and ports, around ppu-core's S-DSP (`Apu`).
//! - `system`: cartridge mapping, WRAM, MMIO ($21xx PPU bridge, $42xx/$43xx), DMA/HDMA, NMI/IRQ,
//!   joypad auto-read, master-clock timing and the frame loop (`Snes`).

pub mod apu;
pub mod cpu;
pub mod system;
#[cfg(all(target_arch = "wasm32", feature = "web"))]
mod wasm;

pub use apu::Apu;
pub use cpu::Cpu;
pub use system::Snes;

/// The 65816's view of the console. Every access advances the master clock by the access's
/// speed (the bus implementation decides 6, 8 or 12 master clocks from the address);
/// `idle` is an internal CPU cycle (6 master clocks).
pub trait Bus {
    fn read(&mut self, addr: u32) -> u8;
    fn write(&mut self, addr: u32, value: u8);
    fn idle(&mut self);
}

/// Controller buttons in the order the auto-read shifts them out (bit 15 first):
/// B, Y, Select, Start, Up, Down, Left, Right, A, X, L, R.
pub mod pad {
    pub const B: u16 = 1 << 15;
    pub const Y: u16 = 1 << 14;
    pub const SELECT: u16 = 1 << 13;
    pub const START: u16 = 1 << 12;
    pub const UP: u16 = 1 << 11;
    pub const DOWN: u16 = 1 << 10;
    pub const LEFT: u16 = 1 << 9;
    pub const RIGHT: u16 = 1 << 8;
    pub const A: u16 = 1 << 7;
    pub const X: u16 = 1 << 6;
    pub const L: u16 = 1 << 5;
    pub const R: u16 = 1 << 4;
}
