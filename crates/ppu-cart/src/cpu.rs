//! The 65816. Owned by the CPU work; the system drives it only through this surface.
use crate::Bus;

#[derive(Clone, Debug, Default)]
pub struct Cpu {
    /// Program bank and counter as one 24-bit address (PBR:PC).
    pub pc: u32,
}

impl Cpu {
    pub fn new() -> Self {
        Self::default()
    }
    /// Power-on/reset: emulation mode, vector at $00:FFFC.
    pub fn reset(&mut self, _bus: &mut impl Bus) {}
    /// Execute one instruction, or service a pending NMI/IRQ first.
    pub fn step(&mut self, _bus: &mut impl Bus) {}
    /// Edge-triggered: the system raises it at the start of VBlank when enabled.
    pub fn nmi(&mut self) {}
    /// Level-triggered IRQ line.
    pub fn set_irq(&mut self, _level: bool) {}
    /// True after WAI until an interrupt; STP stops for good.
    pub fn waiting(&self) -> bool {
        false
    }
}
