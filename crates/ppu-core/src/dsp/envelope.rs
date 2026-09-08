// Ported from snes-apu 0.1.12 (BSD-2-Clause); see mod.rs.
//
// Per-voice ADSR/GAIN envelope. The reference threaded a raw pointer back to
// `Dsp` to read the shared rate-counter table; here `tick` takes the current
// `Dsp` counter as a parameter and calls the free `read_counter` helper in
// `mod.rs` instead.

use super::read_counter;

enum Mode {
    Attack,
    Decay,
    Sustain,
    Release,
}

pub(crate) struct Envelope {
    pub(crate) adsr0: u8,
    pub(crate) adsr1: u8,
    pub(crate) gain: u8,

    mode: Mode,
    pub(crate) level: i32,
    hidden_level: i32,
}

impl Envelope {
    pub(crate) fn new() -> Envelope {
        Envelope {
            adsr0: 0,
            adsr1: 0,
            gain: 0,

            mode: Mode::Release,
            level: 0,
            hidden_level: 0,
        }
    }

    pub(crate) fn key_on(&mut self) {
        self.mode = Mode::Attack;
        self.level = 0;
        self.hidden_level = 0;
    }

    pub(crate) fn key_off(&mut self) {
        self.mode = Mode::Release;
    }

    pub(crate) fn tick(&mut self, counter: i32) {
        let mut env = self.level;
        match self.mode {
            Mode::Release => {
                env -= 8;
                if env < 0 {
                    env = 0;
                }
                self.level = env;
            }
            _ => {
                let rate: i32;
                let env_data = self.adsr1 as i32;
                if (self.adsr0 & 0x80) != 0 {
                    // Adsr mode
                    match self.mode {
                        Mode::Attack => {
                            rate = ((self.adsr0 as i32) & 0x0f) * 2 + 1;
                            env += if rate < 31 { 0x20 } else { 0x400 };
                        }
                        _ => {
                            env -= 1;
                            env -= env >> 8;
                            match self.mode {
                                Mode::Decay => {
                                    rate = (((self.adsr0 as i32) >> 3) & 0x0e) + 0x10;
                                }
                                _ => {
                                    rate = env_data & 0x1f;
                                }
                            }
                        }
                    }
                } else {
                    // Gain mode
                    let mode = self.gain >> 5;
                    if mode < 4 {
                        // Direct
                        env = (self.gain as i32) * 0x10;
                        rate = 31;
                    } else {
                        rate = (self.gain as i32) & 0x1f;
                        if mode == 4 {
                            // Linear decrease
                            env -= 0x20;
                        } else if mode < 6 {
                            // Exponential decrease
                            env -= 1;
                            env -= env >> 8;
                        } else {
                            // Linear increase
                            env += 0x20;
                            if mode > 6 && (self.hidden_level as u32) >= 0x600 {
                                env += 0x08 - 0x20;
                            }
                        }
                    }
                }

                if let Mode::Decay = self.mode {
                    if (env >> 8) == (env_data >> 5) {
                        self.mode = Mode::Sustain;
                    }
                }

                self.hidden_level = env; // Super obscure quirk thingy here

                // Unsigned because env < 0 should also trigger this logic
                if (env as u32) >= 0x07ff {
                    env = if env < 0 { 0 } else { 0x07ff };
                    if let Mode::Attack = self.mode {
                        self.mode = Mode::Decay;
                    }
                }

                if read_counter(counter, rate) {
                    return;
                }
                self.level = env;
            }
        }
    }
}
