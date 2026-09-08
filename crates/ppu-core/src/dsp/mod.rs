// Copyright (c) 2015, Jake "ferris" Taylor
// All rights reserved.
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions are
// met:
//
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the
//    distribution.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
// "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
// LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
// A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
// OWNER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
// LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
// DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
// THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
// (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
// OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
//
// Ported from snes-apu 0.1.12 (BSD-2-Clause), src/dsp/.

//! A pure, headless S-DSP (SNES sound chip) emulator: 128 register bytes,
//! eight voices, BRR sample decode, 4-point Gaussian interpolation, ADSR/GAIN
//! envelopes, noise, pitch modulation, and echo (8-tap FIR + feedback) over a
//! caller-owned 64 KB ARAM array. No `unsafe`, no floating point, no wasm-only
//! code — this module is native-testable and has no crates.io dependencies
//! beyond the rest of `ppu-core`.
//!
//! # Register map
//!
//! Per voice `n` (0..7), registers `n0`..`n9` (e.g. voice 3's VOL(L) is
//! register `0x30`):
//!
//! | Reg | Name       | Meaning                              |
//! |-----|------------|---------------------------------------|
//! | n0  | VOL(L)     | left volume (signed)                   |
//! | n1  | VOL(R)     | right volume (signed)                  |
//! | n2  | PITCH(L)   | pitch low byte                         |
//! | n3  | PITCH(H)   | pitch high byte (6 bits used)           |
//! | n4  | SRCN       | sample source number (index into DIR)  |
//! | n5  | ADSR1 (field `adsr0`) | attack/decay/enable         |
//! | n6  | ADSR2 (field `adsr1`) | decay2/sustain rate         |
//! | n7  | GAIN       | direct gain / envelope mode             |
//! | n8  | ENVX (ro)  | current envelope level (high nibble)   |
//! | n9  | OUTX (ro)  | last output sample (high byte)          |
//!
//! Global registers:
//!
//! | Reg  | Name  | Meaning                                          |
//! |------|-------|---------------------------------------------------|
//! | 0x0C | MVOL(L) | main volume, left (signed)                       |
//! | 0x1C | MVOL(R) | main volume, right (signed)                      |
//! | 0x2C | EVOL(L) | echo volume, left (signed)                       |
//! | 0x3C | EVOL(R) | echo volume, right (signed)                      |
//! | 0x4C | KON     | key-on mask (write, edge-triggered)              |
//! | 0x5C | KOF     | key-off mask (write, edge-triggered)             |
//! | 0x6C | FLG     | bit5 echo-write-disable, bit6 mute, bit7 reset, bits0-4 noise clock |
//! | 0x7C | ENDX    | end-block-reached mask (ro; any write clears all)|
//! | 0x0D | EFB     | echo feedback (signed)                           |
//! | 0x2D | PMON    | pitch-modulation enable mask (voices 1..7)       |
//! | 0x3D | NON     | noise enable mask                                |
//! | 0x4D | EON     | echo enable mask                                 |
//! | 0x5D | DIR     | sample directory page (`DIR << 8`)               |
//! | 0x6D | ESA     | echo buffer start page (`ESA << 8`)              |
//! | 0x7D | EDL     | echo delay, `EDL * 0x800` bytes (EDL=0 is a 4-byte, one-sample buffer at `ESA << 8`, still read and written every tick) |
//! | xF   | FIR c0..c7 | one 8-tap FIR coefficient per voice row (voice index = tap index) |
//!
//! Sample directory: base `DIR << 8`, one 4-byte entry per SRCN
//! (`base + src*4`): little-endian start address at +0, little-endian loop
//! address at +2. Echo buffer: base `ESA << 8`, length `EDL * 0x800` bytes,
//! 4 bytes/sample (L then R, little-endian i16 each, LSB forced to 0).
//!
//! # `Dsp::new()` state
//!
//! Every register-backed field starts at its zero value — this deliberately
//! does *not* replicate the snes-apu-specific power-on defaults (MVOL
//! 0x89/0x9c, EVOL 0x9f/0x9c, the eight FIR coefficients, ESA 0x60, EDL 0x0e,
//! voice pitch_high 0x10), since those are firmware-loaded artifacts, not
//! S-DSP hardware state. The one exception: FLG starts as 0x20 (echo writes
//! off, unmuted, noise clock 0), so a fresh `Dsp` is audible as soon as a
//! voice is keyed on, whereas real hardware reset leaves FLG at 0xE0 (muted).
//! The noise LFSR starts at 0x4000, the rate counter at 0.
//!
//! # KON deferral
//!
//! `write(0x4c, mask)` cannot itself read ARAM (this API's `write` doesn't
//! take one), so it only clears the targeted voices' ENDX bits and marks
//! them pending. KON is applied at the start of the voice's next `render`
//! tick. A KOF written after a KON but before `render` is applied after that
//! deferred KON (matching the reference's key_on-then-key_off: the voice
//! restarts and immediately releases from level 0, i.e. silence). SRCN/DIR/
//! PITCH/ADSR writes between KON and `render` are seen by the deferred
//! KON — the binding should issue KON after all other register writes for
//! the frame. See `voice.rs`.
//!
//! # Mute / reset (FLG bits 6/7)
//!
//! Bit 6 (mute) silences `render`'s output samples but still advances every
//! other piece of state (voices, noise, echo, counter) — see the one `if` in
//! `render`. Bit 7 (soft reset) is stored in `regs` but has no effect; no
//! hardware behavior in this emulator depends on it.

pub(crate) mod brr;
mod echo;
mod envelope;
mod gaussian;
mod voice;

use echo::Filter;
use voice::Voice;

/// Output sample rate, in Hz, of `render`'s interleaved stereo stream.
pub const SAMPLE_RATE: usize = 32000;

const NUM_VOICES: usize = 8;

const COUNTER_RANGE: i32 = 30720;
static COUNTER_RATES: [i32; 32] = [
    COUNTER_RANGE + 1, // Never fires
    2048,
    1536,
    1280,
    1024,
    768,
    640,
    512,
    384,
    320,
    256,
    192,
    160,
    128,
    96,
    80,
    64,
    48,
    40,
    32,
    24,
    20,
    16,
    12,
    10,
    8,
    6,
    5,
    4,
    3,
    2,
    1,
];

static COUNTER_OFFSETS: [i32; 32] = [
    1, 0, 1040, 536, 0, 1040, 536, 0, 1040, 536, 0, 1040, 536, 0, 1040, 536, 0, 1040, 536, 0, 1040,
    536, 0, 1040, 536, 0, 1040, 536, 0, 1040, 0, 0,
];

pub(crate) fn read_counter(counter: i32, rate: i32) -> bool {
    ((counter + COUNTER_OFFSETS[rate as usize]) % COUNTER_RATES[rate as usize]) != 0
}

pub(crate) fn clamp(value: i32) -> i32 {
    if value < -32768 {
        -32768
    } else if value > 32767 {
        32767
    } else {
        value
    }
}

// Signed, unlike snes-apu: hardware VOL/MVOL/EVOL are i8.
pub(crate) fn multiply_volume(value: i32, volume: u8) -> i32 {
    (value * ((volume as i8) as i32)) >> 7
}

/// A headless S-DSP core. See the module docs for the register map and the
/// `new()`/KON/mute notes.
///
/// Global registers (MVOL/EVOL/FLG/EFB/DIR/ESA/EDL) are read straight out of
/// `regs` in `render`; per-voice registers and FIR coefficients are cached
/// on `Voice`/`Filter` at write time instead. `write` is the only mutator
/// for both, so the cache and `regs` can never drift apart.
pub struct Dsp {
    regs: [u8; 128],

    voices: [Voice; NUM_VOICES],

    left_filter: Filter,
    right_filter: Filter,

    /// The live ENDX mask. `write(0x7c, _)` still stores the written byte
    /// into `regs[0x7c]` (as does any read-only n8/n9 write), but that copy
    /// is never read back — `regs[0x7c]` is inert; `read(0x7c)` and every
    /// ENDX update go through this field instead.
    endx: u8,

    counter: i32,
    noise: i32,
    echo_pos: i32,
    echo_length: i32,
}

impl Dsp {
    /// See the module doc comment's "`Dsp::new()` state" section.
    pub fn new() -> Dsp {
        let mut regs = [0u8; 128];
        regs[0x6c] = 0x20;
        Dsp {
            regs,
            voices: std::array::from_fn(|_| Voice::new()),
            left_filter: Filter::new(),
            right_filter: Filter::new(),
            endx: 0,
            counter: 0,
            noise: 0x4000,
            echo_pos: 0,
            echo_length: 0,
        }
    }

    fn set_filter_coefficient(&mut self, index: usize, value: u8) {
        self.left_filter.coefficients[index] = value;
        self.right_filter.coefficients[index] = value;
    }

    fn set_kon(&mut self, voice_mask: u8) {
        for i in 0..NUM_VOICES {
            if (voice_mask & (1 << i)) != 0 {
                self.voices[i].request_key_on();
                self.endx &= !(1 << i);
            }
        }
    }

    fn set_kof(&mut self, voice_mask: u8) {
        for i in 0..NUM_VOICES {
            if (voice_mask & (1 << i)) != 0 {
                self.voices[i].key_off();
            }
        }
    }

    fn set_pmon(&mut self, voice_mask: u8) {
        for i in 1..NUM_VOICES {
            self.voices[i].pitch_mod = (voice_mask & (1 << i)) != 0;
        }
    }

    fn set_non(&mut self, voice_mask: u8) {
        for i in 0..NUM_VOICES {
            self.voices[i].noise_on = (voice_mask & (1 << i)) != 0;
        }
    }

    fn set_eon(&mut self, voice_mask: u8) {
        for i in 0..NUM_VOICES {
            self.voices[i].echo_on = (voice_mask & (1 << i)) != 0;
        }
    }

    /// Write one S-DSP register. Ignored if `reg & 0x80 != 0` (matching the
    /// reference); otherwise the raw byte is stored in `regs` and any
    /// register-specific side effect (voice fields, FIR coefficient, KON/KOF,
    /// PMON/NON/EON masks, ENDX clear) is applied.
    pub fn write(&mut self, reg: u8, val: u8) {
        if reg & 0x80 != 0 {
            return;
        }
        self.regs[reg as usize] = val;

        let voice_index = (reg >> 4) as usize;
        let voice_addr = reg & 0x0f;
        if voice_addr < 0x0a {
            if voice_addr < 8 {
                let voice = &mut self.voices[voice_index];
                match voice_addr {
                    0x00 => voice.vol_left = val,
                    0x01 => voice.vol_right = val,
                    0x02 => voice.pitch_low = val,
                    0x03 => voice.set_pitch_high(val),
                    0x04 => voice.source = val,
                    0x05 => voice.envelope.adsr0 = val,
                    0x06 => voice.envelope.adsr1 = val,
                    0x07 => voice.envelope.gain = val,
                    _ => unreachable!(),
                }
            }
        } else if voice_addr == 0x0f {
            self.set_filter_coefficient(voice_index, val);
        } else {
            match reg {
                0x4c => self.set_kon(val),
                0x5c => self.set_kof(val),
                0x7c => self.endx = 0,
                0x2d => self.set_pmon(val),
                0x3d => self.set_non(val),
                0x4d => self.set_eon(val),
                // 0x0c/1c/2c/3c (MVOL/EVOL), 0x6c (FLG), 0x0d (EFB), 0x5d
                // (DIR), 0x6d (ESA), 0x7d (EDL): no cached copy needed, read
                // straight out of `regs` in `render`.
                _ => (),
            }
        }
    }

    /// Read one S-DSP register. `regs` is authoritative except the two
    /// read-only per-voice values (ENVX, OUTX) and ENDX, which are derived
    /// live.
    pub fn read(&self, reg: u8) -> u8 {
        let reg = reg & 0x7f;
        if reg == 0x7c {
            return self.endx;
        }
        let voice_index = (reg >> 4) as usize;
        match reg & 0x0f {
            0x08 => (self.voices[voice_index].envelope.level >> 4) as u8,
            0x09 => self.voices[voice_index].outx,
            _ => self.regs[reg as usize],
        }
    }

    /// Render `out.len() / 2` interleaved stereo frames at [`SAMPLE_RATE`].
    /// `out` holds `frames * 2` `i16`s (left, right per frame); an odd
    /// trailing element is left untouched. Reads and writes `aram` for BRR
    /// sample data, the sample directory, and the echo buffer.
    pub fn render(&mut self, aram: &mut [u8; 0x10000], out: &mut [i16]) {
        let num_frames = out.len() / 2;
        for frame in 0..num_frames {
            let counter = self.counter;
            let noise_clock = (self.regs[0x6c] & 0x1f) as i32;
            if !read_counter(counter, noise_clock) {
                let feedback = (self.noise << 13) ^ (self.noise << 14);
                self.noise = (feedback & 0x4000) ^ (self.noise >> 1);
            }

            let source_dir = self.regs[0x5d];
            let mut left_out = 0i32;
            let mut right_out = 0i32;
            let mut left_echo_out = 0i32;
            let mut right_echo_out = 0i32;
            let mut last_voice_out = 0i32;
            let mut end_flags = [false; NUM_VOICES];
            for i in 0..NUM_VOICES {
                let voice = &mut self.voices[i];
                let output =
                    voice.render_sample(aram, source_dir, counter, last_voice_out, self.noise);

                left_out = clamp(left_out + output.left_out);
                right_out = clamp(right_out + output.right_out);

                if voice.echo_on {
                    left_echo_out = clamp(left_echo_out + output.left_out);
                    right_echo_out = clamp(right_echo_out + output.right_out);
                }

                last_voice_out = output.last_voice_out;
                end_flags[i] = voice.end_flag();
            }
            for (i, &ended) in end_flags.iter().enumerate() {
                if ended {
                    self.endx |= 1 << i;
                }
            }

            left_out = multiply_volume(left_out, self.regs[0x0c]);
            right_out = multiply_volume(right_out, self.regs[0x1c]);

            // Wraps at 16 bits (`u16`, not the reference's `u32`) — ESA/echo_pos
            // can never index past `aram`'s 64 KB.
            let esa = (self.regs[0x6d] as u16) << 8;
            let echo_addr = esa.wrapping_add(self.echo_pos as u16);
            let b0 = aram[echo_addr as usize] as i32;
            let b1 = aram[echo_addr.wrapping_add(1) as usize] as i32;
            let b2 = aram[echo_addr.wrapping_add(2) as usize] as i32;
            let b3 = aram[echo_addr.wrapping_add(3) as usize] as i32;
            let mut left_echo_in = ((((b1 << 8) | b0) as i16) & !1) as i32;
            let mut right_echo_in = ((((b3 << 8) | b2) as i16) & !1) as i32;

            left_echo_in = clamp(self.left_filter.next(left_echo_in));
            right_echo_in = clamp(self.right_filter.next(right_echo_in));

            let left_final =
                clamp(left_out + multiply_volume(left_echo_in, self.regs[0x2c])) as i16;
            let right_final =
                clamp(right_out + multiply_volume(right_echo_in, self.regs[0x3c])) as i16;

            let flg = self.regs[0x6c];
            if flg & 0x40 != 0 {
                out[frame * 2] = 0;
                out[frame * 2 + 1] = 0;
            } else {
                out[frame * 2] = left_final;
                out[frame * 2 + 1] = right_final;
            }

            if flg & 0x20 == 0 {
                let echo_feedback = self.regs[0x0d] as i8 as i32;
                let left_echo_write =
                    clamp(left_echo_out + ((((left_echo_in * echo_feedback) >> 7) as i16) as i32))
                        & !1;
                let right_echo_write = clamp(
                    right_echo_out + ((((right_echo_in * echo_feedback) >> 7) as i16) as i32),
                ) & !1;

                aram[echo_addr as usize] = left_echo_write as u8;
                aram[echo_addr.wrapping_add(1) as usize] = (left_echo_write >> 8) as u8;
                aram[echo_addr.wrapping_add(2) as usize] = right_echo_write as u8;
                aram[echo_addr.wrapping_add(3) as usize] = (right_echo_write >> 8) as u8;
            }

            if self.echo_pos == 0 {
                let edl = self.regs[0x7d] & 0x0f;
                self.echo_length = (edl as i32) * 0x800;
            }
            self.echo_pos += 4;
            if self.echo_pos >= self.echo_length {
                self.echo_pos = 0;
            }

            self.counter = (self.counter + 1) % COUNTER_RANGE;
        }
    }
}

impl Default for Dsp {
    fn default() -> Self {
        Dsp::new()
    }
}
