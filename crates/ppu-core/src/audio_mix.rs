//! Post-sequencer DSP adjustments. Never modify Lua's song-owned registers.
use crate::{Dsp, DspSampleView};
use serde::{Deserialize, Serialize};

pub const AUDIO_MIX_FILE: &str = "audio.mix.json";

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct VoiceMix {
    pub left_db: f32,
    pub right_db: f32,
    pub transpose: f32,
    pub mute: bool,
    pub solo: bool,
    pub sample: Option<u8>,
    pub pitch: Option<u16>,
    pub left: Option<i8>,
    pub right: Option<i8>,
    pub adsr: Option<[u8; 4]>,
    pub gain: Option<u8>,
    pub echo: Option<bool>,
    pub noise: Option<bool>,
    pub pmod: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct AudioMix {
    pub voices: [VoiceMix; 8],
    pub master_db: f32,
    pub echo_db: f32,
    pub master_left: Option<i8>,
    pub master_right: Option<i8>,
    pub echo_left: Option<i8>,
    pub echo_right: Option<i8>,
    pub echo_delay: Option<u8>,
    pub feedback: Option<i8>,
    pub fir: Option<[i8; 8]>,
    pub noise_clock: Option<u8>,
}

impl AudioMix {
    pub fn parse(json: &str) -> Result<Self, String> {
        if json.len() > 16384 {
            return Err("Audio mix exceeds 16 KB".into());
        }
        let mix: Self = serde_json::from_str(json).map_err(|e| format!("Audio mix: {e}"))?;
        let db = |v: f32| v.is_finite() && (-60.0..=12.0).contains(&v);
        if !db(mix.master_db)
            || !db(mix.echo_db)
            || mix.echo_delay.is_some_and(|v| v > 15)
            || mix.noise_clock.is_some_and(|v| v > 31)
        {
            return Err("Audio mix: invalid master/echo/noise value".into());
        }
        for v in &mix.voices {
            if !db(v.left_db)
                || !db(v.right_db)
                || !v.transpose.is_finite()
                || !(-48.0..=48.0).contains(&v.transpose)
                || v.pitch.is_some_and(|v| v > 0x3fff)
                || v.adsr
                    .is_some_and(|a| a[0] > 15 || a[1] > 7 || a[2] > 7 || a[3] > 31)
                || (v.adsr.is_some() && v.gain.is_some())
            {
                return Err("Audio mix: invalid voice trim or override".into());
            }
        }
        Ok(mix)
    }

    pub fn validate_samples(&self, samples: &[DspSampleView]) -> Result<(), String> {
        if let Some(delay) = self.echo_delay {
            let start = if delay == 0 {
                0xff00
            } else {
                0x10000 - u32::from(delay) * 0x800
            };
            let end = if delay == 0 { 0xff04 } else { 0x10000 };
            if let Some(s) = samples.iter().find(|s| s.start < end && start < s.end) {
                return Err(format!(
                    "Echo delay overlaps sample '{}' in sound RAM",
                    s.name
                ));
            }
        }
        Ok(())
    }

    pub fn apply(&self, dsp: &mut Dsp) {
        fn volume(dsp: &mut Dsp, reg: u8, pinned: Option<i8>, db: f32) {
            let value = pinned.unwrap_or(dsp.read(reg) as i8);
            let scaled = (f32::from(value) * 10.0_f32.powf(db / 20.0))
                .round()
                .clamp(-128.0, 127.0);
            dsp.write(reg, scaled as i8 as u8);
        }
        fn flag(dsp: &mut Dsp, reg: u8, bit: u8, value: Option<bool>) {
            if let Some(on) = value {
                dsp.write(reg, (dsp.read(reg) & !(1 << bit)) | ((on as u8) << bit));
            }
        }
        let solo = self.voices.iter().any(|v| v.solo);
        for (n, v) in self.voices.iter().enumerate() {
            let base = (n as u8) << 4;
            volume(dsp, base, v.left, v.left_db);
            volume(dsp, base | 1, v.right, v.right_db);
            if v.mute || (solo && !v.solo) {
                dsp.write(base, 0);
                dsp.write(base | 1, 0);
            }
            let pitch = v
                .pitch
                .unwrap_or(u16::from(dsp.read(base | 2)) | (u16::from(dsp.read(base | 3)) << 8));
            let pitch = (f32::from(pitch) * 2.0_f32.powf(v.transpose / 12.0))
                .round()
                .clamp(0.0, 16383.0) as u16;
            dsp.write(base | 2, pitch as u8);
            dsp.write(base | 3, (pitch >> 8) as u8);
            if let Some(sample) = v.sample {
                dsp.write(base | 4, sample);
            }
            if let Some([a, d, s, r]) = v.adsr {
                dsp.write(base | 5, 0x80 | (d << 4) | a);
                dsp.write(base | 6, (s << 5) | r);
            }
            if let Some(gain) = v.gain {
                dsp.write(base | 5, dsp.read(base | 5) & 0x7f);
                dsp.write(base | 7, gain);
            }
            flag(dsp, 0x4d, n as u8, v.echo);
            flag(dsp, 0x3d, n as u8, v.noise);
            if n > 0 {
                flag(dsp, 0x2d, n as u8, v.pmod);
            }
        }
        volume(dsp, 0x0c, self.master_left, self.master_db);
        volume(dsp, 0x1c, self.master_right, self.master_db);
        volume(dsp, 0x2c, self.echo_left, self.echo_db);
        volume(dsp, 0x3c, self.echo_right, self.echo_db);
        if let Some(delay) = self.echo_delay {
            dsp.write(0x7d, delay);
            dsp.write(
                0x6d,
                if delay == 0 {
                    0xff
                } else {
                    (256 - u16::from(delay) * 8) as u8
                },
            );
            dsp.write(
                0x6c,
                (dsp.read(0x6c) & !0x20) | if delay == 0 { 0x20 } else { 0 },
            );
        }
        if let Some(v) = self.feedback {
            dsp.write(0x0d, v as u8);
        }
        if let Some(fir) = self.fir {
            for (i, v) in fir.into_iter().enumerate() {
                dsp.write((i as u8) << 4 | 0x0f, v as u8);
            }
        }
        if let Some(v) = self.noise_clock {
            dsp.write(0x6c, (dsp.read(0x6c) & !31) | v);
        }
    }
}
