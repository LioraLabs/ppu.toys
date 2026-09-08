// Ported from snes-apu 0.1.12 (BSD-2-Clause); see mod.rs.
//
// Per-voice state: BRR sample playback, 4-point Gaussian resampling, and the
// register-mapped fields (VOL/PITCH/SRCN/ADSR/GAIN/ENVX/OUTX).
//
// KON deferral (forced by this crate's API split between `Dsp::write`, which
// has no ARAM access, and `Dsp::render`, which does): a KON write only sets
// `pending_kon`; the actual directory read + first BRR block decode (the
// reference's `Voice::key_on`) is applied at the start of this voice's next
// `render_sample` call, which is the first point ARAM is available. A KOF
// written after a KON but before that `render_sample` call is itself
// deferred (`pending_kof`) and applied right after the deferred KON, so the
// order matches the reference's key_on-then-key_off: the voice restarts and
// immediately releases from level 0, i.e. silence.

use super::brr::BrrBlockDecoder;
use super::envelope::Envelope;
use super::gaussian::{HALF_KERNEL, HALF_KERNEL_SIZE};
use super::{clamp, multiply_volume};

const RESAMPLE_BUFFER_LEN: usize = 4;

pub(crate) struct VoiceOutput {
    pub(crate) left_out: i32,
    pub(crate) right_out: i32,
    pub(crate) last_voice_out: i32,
}

pub(crate) struct Voice {
    pub(crate) envelope: Envelope,

    pub(crate) vol_left: u8,
    pub(crate) vol_right: u8,
    pub(crate) pitch_low: u8,
    pitch_high: u8,
    pub(crate) source: u8,
    pub(crate) pitch_mod: bool,
    pub(crate) noise_on: bool,
    pub(crate) echo_on: bool,
    pub(crate) outx: u8,
    pending_kon: bool,
    pending_kof: bool,

    sample_start_address: u16,
    loop_start_address: u16,
    brr_block_decoder: BrrBlockDecoder,
    sample_address: u16,
    sample_pos: i32,

    resample_buffer: [i32; RESAMPLE_BUFFER_LEN],
    resample_buffer_pos: usize,
}

/// Sample directory lookup: `DIR << 8` is the directory base, each entry is 4
/// bytes (`src * 4` from the base): a little-endian start address at +0 and a
/// little-endian loop address at +2. All arithmetic wraps at 16 bits so a
/// pathological DIR/SRCN pair can never index out of `aram`.
fn read_dir_address(aram: &[u8; 0x10000], dir: u8, src: u8, offset: u16) -> u16 {
    let entry = ((dir as u16) << 8)
        .wrapping_add((src as u16).wrapping_mul(4))
        .wrapping_add(offset);
    let lo = aram[entry as usize] as u16;
    let hi = aram[entry.wrapping_add(1) as usize] as u16;
    lo | (hi << 8)
}

impl Voice {
    pub(crate) fn new() -> Voice {
        Voice {
            envelope: Envelope::new(),

            vol_left: 0,
            vol_right: 0,
            pitch_low: 0,
            pitch_high: 0,
            source: 0,
            pitch_mod: false,
            noise_on: false,
            echo_on: false,
            outx: 0,
            pending_kon: false,
            pending_kof: false,

            sample_start_address: 0,
            loop_start_address: 0,
            brr_block_decoder: BrrBlockDecoder::new(),
            sample_address: 0,
            sample_pos: 0,

            resample_buffer: [0; RESAMPLE_BUFFER_LEN],
            resample_buffer_pos: 0,
        }
    }

    pub(crate) fn render_sample(
        &mut self,
        aram: &[u8; 0x10000],
        source_dir: u8,
        counter: i32,
        last_voice_out: i32,
        noise: i32,
    ) -> VoiceOutput {
        if self.pending_kon {
            self.pending_kon = false;
            self.key_on(aram, source_dir);
        }
        if self.pending_kof {
            self.pending_kof = false;
            self.envelope.key_off();
        }

        let mut pitch = ((self.pitch_high as i32) << 8) | (self.pitch_low as i32);
        if self.pitch_mod {
            pitch += ((last_voice_out >> 5) * pitch) >> 10;
        }
        if pitch < 0 {
            pitch = 0;
        }
        if pitch > 0x3fff {
            pitch = 0x3fff;
        }

        let mut sample = if !self.noise_on {
            let s1 = self.resample_buffer[self.resample_buffer_pos];
            let s2 = self.resample_buffer[(self.resample_buffer_pos + 1) % RESAMPLE_BUFFER_LEN];
            let s3 = self.resample_buffer[(self.resample_buffer_pos + 2) % RESAMPLE_BUFFER_LEN];
            let s4 = self.resample_buffer[(self.resample_buffer_pos + 3) % RESAMPLE_BUFFER_LEN];
            let kernel_index = (self.sample_pos >> 2) as usize;
            let p1 = HALF_KERNEL[kernel_index] as i32;
            let p2 = HALF_KERNEL[kernel_index + HALF_KERNEL_SIZE / 2] as i32;
            let p3 = HALF_KERNEL[HALF_KERNEL_SIZE - 1 - kernel_index] as i32;
            let p4 =
                HALF_KERNEL[HALF_KERNEL_SIZE - 1 - (kernel_index + HALF_KERNEL_SIZE / 2)] as i32;
            let resampled = (s1 * p1 + s2 * p2 + s3 * p3 + s4 * p4) >> 11;
            clamp(resampled) & !1
        } else {
            ((noise * 2) as i16) as i32
        };

        self.envelope.tick(counter);
        let env_level = self.envelope.level;

        sample = ((sample * env_level) >> 11) & !1;
        self.outx = (sample >> 8) as u8;

        if self.brr_block_decoder.is_end && !self.brr_block_decoder.is_looping {
            self.envelope.key_off();
            self.envelope.level = 0;
        }

        self.sample_pos += pitch;
        while self.sample_pos >= 0x1000 {
            self.sample_pos -= 0x1000;
            self.read_next_sample();

            if self.brr_block_decoder.is_finished() {
                if self.brr_block_decoder.is_end && self.brr_block_decoder.is_looping {
                    self.read_entry(aram, source_dir);
                    self.sample_address = self.loop_start_address;
                }
                self.read_next_block(aram);
            }
        }

        VoiceOutput {
            left_out: multiply_volume(sample, self.vol_left),
            right_out: multiply_volume(sample, self.vol_right),
            last_voice_out: sample,
        }
    }

    /// True the tick a BRR block with the end bit was decoded (sticky for as
    /// long as the current block's header set it) — the caller ORs this into
    /// Dsp's ENDX mask.
    pub(crate) fn end_flag(&self) -> bool {
        self.brr_block_decoder.is_end
    }

    pub(crate) fn set_pitch_high(&mut self, value: u8) {
        self.pitch_high = value & 0x3f;
    }

    /// In the reference, KON runs `key_on` synchronously, so a KOF written
    /// after a KON but before the emulator's next tick lands on an
    /// already-restarted envelope (Attack) and immediately releases it —
    /// silence. Here KON is deferred (see `request_key_on`), so a KOF that
    /// arrives while a KON is still pending must itself wait: it's recorded
    /// and replayed right after the deferred `key_on` in `render_sample`,
    /// reproducing the same restart-then-release order.
    pub(crate) fn key_off(&mut self) {
        if self.pending_kon {
            self.pending_kof = true;
        }
        self.envelope.key_off();
    }

    /// Request a key-on. The reference's immediate directory read + first
    /// block decode is deferred to this voice's next `render_sample`, since
    /// `Dsp::write` (where KON arrives) has no ARAM access. Clears any KOF
    /// that was pending behind a still-unapplied KON: `KON; KOF; KON` in the
    /// reference plays (the second KON re-restarts the envelope after the
    /// first's KOF took effect), so a fresh KON here must discard the queued
    /// KOF rather than let it fire after this KON's deferred `key_on`.
    pub(crate) fn request_key_on(&mut self) {
        self.pending_kon = true;
        self.pending_kof = false;
    }

    fn key_on(&mut self, aram: &[u8; 0x10000], source_dir: u8) {
        self.read_entry(aram, source_dir);
        self.sample_address = self.sample_start_address;
        self.brr_block_decoder.reset(0, 0);
        self.read_next_block(aram);
        self.sample_pos = 0;
        for i in 0..RESAMPLE_BUFFER_LEN {
            self.resample_buffer[i] = 0;
        }
        self.read_next_sample();
        self.envelope.key_on();
    }

    fn read_entry(&mut self, aram: &[u8; 0x10000], source_dir: u8) {
        self.sample_start_address = read_dir_address(aram, source_dir, self.source, 0);
        self.loop_start_address = read_dir_address(aram, source_dir, self.source, 2);
    }

    fn read_next_block(&mut self, aram: &[u8; 0x10000]) {
        let mut buf = [0u8; 9];
        for (i, b) in buf.iter_mut().enumerate() {
            *b = aram[self.sample_address.wrapping_add(i as u16) as usize];
        }
        self.brr_block_decoder.read(&buf);
        self.sample_address = self.sample_address.wrapping_add(9);
    }

    fn read_next_sample(&mut self) {
        self.resample_buffer_pos = match self.resample_buffer_pos {
            0 => RESAMPLE_BUFFER_LEN - 1,
            x => x - 1,
        };
        self.resample_buffer[self.resample_buffer_pos] =
            self.brr_block_decoder.read_next_sample() as i32;
    }
}
