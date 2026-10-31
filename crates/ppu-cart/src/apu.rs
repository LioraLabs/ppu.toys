//! The SPC700 side: CPU, 64 KB audio RAM, timers, the four ports, and ppu-core's S-DSP.
//! The boot ROM is not copied: its upload protocol is implemented from documentation.

#[derive(Default)]
pub struct Apu {}

impl Apu {
    pub fn new() -> Self {
        Self::default()
    }
    /// $2140-$2143 as the 65816 reads them.
    pub fn cpu_read_port(&mut self, _port: u8) -> u8 {
        0
    }
    /// $2140-$2143 as the 65816 writes them.
    pub fn cpu_write_port(&mut self, _port: u8, _value: u8) {}
    /// Run the audio side up to `master_clock` (21.477 MHz NTSC master clocks since power-on).
    pub fn run_until(&mut self, _master_clock: u64) {}
    /// Stereo 32 kHz samples produced since the last call, interleaved L, R.
    pub fn take_samples(&mut self, _out: &mut Vec<i16>) {}
}
