//! `ppu sfx`: a small synth that rolls a sound effect from a preset and a seed, reshaped by
//! knobs in real units (Hz, milliseconds, semitones), out as a mono [`crate::wav::DSP_RATE`]
//! WAV that converts as a sample source. Same request, same bytes.
//!
//! A preset is a [`Shape`]: one oscillator, a linear attack / sustain / decay envelope, and a
//! pitch motion (a glide, an optional floor, an optional arpeggio jump, an optional repeat) whose
//! times are fractions of the sound, so the motion stretches with the envelope.

use crate::wav::DSP_RATE;

/// The longest sound, in ms, that fits a sample source (whole BRR blocks, see
/// [`crate::MAX_SAMPLE_BLOCKS`]).
pub const MAX_MS: u32 = (crate::MAX_SAMPLE_BLOCKS * 16 * 1000 / DSP_RATE as usize) as u32;
pub const PITCH_HZ: (f64, f64) = (30.0, 8000.0);
pub const LENGTH_MS: (f64, f64) = (10.0, MAX_MS as f64);
pub const STAGE_MS: (f64, f64) = (0.0, MAX_MS as f64);
pub const SWEEP_ST: (f64, f64) = (-72.0, 72.0);
pub const DEFAULT_SEED: u64 = 1;
/// Highest pitch the oscillator plays; a glide stops here (just under Nyquist's half).
const CEILING_HZ: f64 = 8000.0;
const FLOOR_HZ: f64 = 20.0;
/// Noise holds this many random levels per cycle, so its pitch is its grain.
const NOISE_STEPS: f64 = 16.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    Coin,
    Jump,
    Hurt,
    Explosion,
    Powerup,
    Blip,
    Laser,
}

pub const PRESETS: [(&str, Preset); 7] = [
    ("coin", Preset::Coin),
    ("jump", Preset::Jump),
    ("hurt", Preset::Hurt),
    ("explosion", Preset::Explosion),
    ("powerup", Preset::Powerup),
    ("blip", Preset::Blip),
    ("laser", Preset::Laser),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wave {
    Square,
    Saw,
    Sine,
    Noise,
}

const WAVES: [(&str, Wave); 4] = [
    ("square", Wave::Square),
    ("saw", Wave::Saw),
    ("sine", Wave::Sine),
    ("noise", Wave::Noise),
];

fn wave_name(w: Wave) -> &'static str {
    WAVES.iter().find(|(_, x)| *x == w).unwrap().0
}

fn preset_name(p: Preset) -> &'static str {
    PRESETS.iter().find(|(_, x)| *x == p).unwrap().0
}

/// What a preset rolled: everything the renderer plays.
#[derive(Clone, Debug, PartialEq)]
pub struct Shape {
    pub wave: Wave,
    /// Square duty, 0..1.
    pub duty: f64,
    pub pitch_hz: f64,
    pub attack_ms: f64,
    pub sustain_ms: f64,
    pub decay_ms: f64,
    /// Glide travel over one motion cycle (the whole sound, or one repeat period).
    pub sweep_st: f64,
    /// Lowest pitch of a downward glide, in semitones from the start.
    pub floor_st: Option<f64>,
    /// (when, as a fraction of the motion cycle; jump in semitones).
    pub arp: Option<(f64, f64)>,
    /// Motion restarts every this fraction of the sound.
    pub repeat: Option<f64>,
}

impl Shape {
    fn length_ms(&self) -> f64 {
        self.attack_ms + self.sustain_ms + self.decay_ms
    }
}

/// The knobs a request may set; `None` keeps the preset's.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Knobs {
    pub wave: Option<Wave>,
    pub pitch_hz: Option<f64>,
    pub length_ms: Option<f64>,
    pub attack_ms: Option<f64>,
    pub decay_ms: Option<f64>,
    pub sweep_st: Option<f64>,
    pub seed: Option<u64>,
}

/// A parsed `ppu sfx` command line.
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub preset: Preset,
    pub knobs: Knobs,
    pub out: Option<String>,
}

/// SplitMix64: the preset's dice. Small, and the same everywhere.
struct Dice(u64);

impl Dice {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    /// Uniform in [lo, hi).
    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
    fn chance(&mut self, p: f64) -> bool {
        self.range(0.0, 1.0) < p
    }
    fn pick<T: Copy>(&mut self, xs: &[T]) -> T {
        xs[(self.next() % xs.len() as u64) as usize]
    }
}

/// Roll a preset's shape from a seed.
pub fn roll(preset: Preset, seed: u64) -> Shape {
    let mut d = Dice(seed);
    let base = |wave, pitch_hz, sustain_ms, decay_ms| Shape {
        wave,
        duty: 0.5,
        pitch_hz,
        attack_ms: 0.0,
        sustain_ms,
        decay_ms,
        sweep_st: 0.0,
        floor_st: None,
        arp: None,
        repeat: None,
    };
    match preset {
        Preset::Coin => {
            let mut s = base(
                Wave::Square,
                d.range(900.0, 1500.0),
                d.range(40.0, 90.0),
                d.range(150.0, 320.0),
            );
            s.duty = d.pick(&[0.5, 0.25]);
            if d.chance(0.85) {
                s.arp = Some((d.range(0.15, 0.35), d.pick(&[5.0, 7.0, 12.0])));
            }
            s
        }
        Preset::Jump => {
            let mut s = base(
                Wave::Square,
                d.range(220.0, 450.0),
                d.range(30.0, 70.0),
                d.range(100.0, 200.0),
            );
            s.duty = d.pick(&[0.5, 0.25, 0.125]);
            s.sweep_st = d.range(7.0, 14.0);
            s
        }
        Preset::Hurt => {
            let wave = d.pick(&[Wave::Square, Wave::Sine, Wave::Noise]);
            let pitch = if wave == Wave::Noise {
                d.range(1200.0, 2600.0)
            } else {
                d.range(250.0, 700.0)
            };
            let mut s = base(wave, pitch, d.range(20.0, 60.0), d.range(100.0, 220.0));
            s.sweep_st = -d.range(10.0, 20.0);
            s
        }
        Preset::Explosion => {
            let mut s = base(
                Wave::Noise,
                d.range(60.0, 160.0),
                d.range(60.0, 160.0),
                d.range(400.0, 900.0),
            );
            s.sweep_st = -d.range(5.0, 12.0);
            s
        }
        Preset::Powerup => {
            let mut s = base(
                d.pick(&[Wave::Square, Wave::Saw]),
                d.range(280.0, 520.0),
                d.range(150.0, 300.0),
                d.range(150.0, 300.0),
            );
            if d.chance(0.5) {
                s.repeat = Some(1.0 / d.pick(&[3.0, 4.0, 5.0]));
                s.sweep_st = d.range(5.0, 12.0);
            } else {
                s.sweep_st = d.range(12.0, 24.0);
            }
            s
        }
        Preset::Blip => {
            let mut s = base(
                d.pick(&[Wave::Square, Wave::Sine]),
                d.range(800.0, 1600.0),
                d.range(15.0, 40.0),
                d.range(20.0, 60.0),
            );
            s.attack_ms = 2.0;
            s
        }
        Preset::Laser => {
            let mut s = base(
                d.pick(&[Wave::Square, Wave::Saw]),
                d.range(1400.0, 2600.0),
                d.range(80.0, 150.0),
                d.range(80.0, 180.0),
            );
            s.duty = d.pick(&[0.5, 0.25]);
            s.sweep_st = -d.range(24.0, 36.0);
            s.floor_st = Some(-d.range(14.0, 20.0));
            s
        }
    }
}

fn out_of_range(name: &str, unit: &str, v: f64, (lo, hi): (f64, f64)) -> Option<String> {
    (!(lo..=hi).contains(&v)).then(|| format!("{name} {v} {unit} is out of range: {lo} to {hi}"))
}

/// Apply the knobs to a preset's shape. Flags pin envelope stages exactly; `--length` alone
/// scales the shape; `--length` with pins shares what is left among the unpinned stages in the
/// preset's proportions. `--sweep` replaces the glide and drops the floor.
pub fn shape(preset: Preset, knobs: &Knobs) -> Result<Shape, String> {
    let mut s = roll(preset, knobs.seed.unwrap_or(DEFAULT_SEED));
    if let Some(w) = knobs.wave {
        s.wave = w;
    }
    if let Some(p) = knobs.pitch_hz {
        s.pitch_hz = p; // the floor is in semitones from the start, so it moves along
    }
    if let Some(st) = knobs.sweep_st {
        s.sweep_st = st;
        s.floor_st = None;
    }
    match (knobs.length_ms, knobs.attack_ms, knobs.decay_ms) {
        (None, a, d) => {
            s.attack_ms = a.unwrap_or(s.attack_ms);
            s.decay_ms = d.unwrap_or(s.decay_ms);
        }
        (Some(len), None, None) => {
            let k = len / s.length_ms();
            s.attack_ms *= k;
            s.sustain_ms *= k;
            s.decay_ms *= k;
        }
        (Some(len), a, d) => {
            let pinned = a.unwrap_or(0.0) + d.unwrap_or(0.0);
            if pinned > len {
                return Err(format!(
                    "--attack {} ms + --decay {} ms is longer than --length {len} ms",
                    a.unwrap_or(0.0),
                    d.unwrap_or(0.0)
                ));
            }
            let free = [
                if a.is_none() { s.attack_ms } else { 0.0 },
                s.sustain_ms,
                if d.is_none() { s.decay_ms } else { 0.0 },
            ];
            let total: f64 = free.iter().sum();
            let share = |x: f64| {
                if total > 0.0 {
                    x / total * (len - pinned)
                } else {
                    0.0
                }
            };
            s.attack_ms = a.unwrap_or_else(|| share(free[0]));
            s.decay_ms = d.unwrap_or_else(|| share(free[2]));
            s.sustain_ms = if total > 0.0 {
                share(free[1])
            } else {
                len - pinned
            };
        }
    }
    if s.length_ms() > MAX_MS as f64 {
        return Err(format!(
            "the sound would be {:.0} ms ({:.0} attack + {:.0} sustain + {:.0} decay), past the \
             {MAX_MS} ms a sample source holds; set --length",
            s.length_ms(),
            s.attack_ms,
            s.sustain_ms,
            s.decay_ms
        ));
    }
    if s.length_ms() * DSP_RATE as f64 / 1000.0 < 1.0 {
        return Err("the sound would be shorter than one sample; set --length".into());
    }
    Ok(s)
}

/// What rendered, measured from the result rather than echoed from the request.
#[derive(Clone, Debug, PartialEq)]
pub struct Stats {
    pub preset: Preset,
    pub seed: u64,
    pub wave: Wave,
    pub pitch_hz: f64,
    /// Glide travel over one motion cycle as played (a floor or the ceiling can cut it short).
    pub sweep_st: f64,
    pub attack_ms: f64,
    pub sustain_ms: f64,
    pub decay_ms: f64,
    pub samples: usize,
    /// Loudest sample against full scale.
    pub peak_dbfs: Option<f64>,
}

impl Stats {
    pub fn to_json(&self) -> serde_json::Value {
        let r = |x: f64, k: f64| (x * k).round() / k;
        serde_json::json!({
            "preset": preset_name(self.preset),
            "seed": self.seed,
            "wave": wave_name(self.wave),
            "pitch_hz": r(self.pitch_hz, 100.0),
            "sweep_semitones": r(self.sweep_st, 100.0),
            "attack_ms": r(self.attack_ms, 100.0),
            "sustain_ms": r(self.sustain_ms, 100.0),
            "decay_ms": r(self.decay_ms, 100.0),
            "duration_seconds": r(self.samples as f64 / DSP_RATE as f64, 100000.0),
            "samples": self.samples,
            "sample_rate": DSP_RATE,
            "peak_dbfs": self.peak_dbfs.map(|p| r(p, 100.0)),
        })
    }
}

/// Render a shape to mono i16 at [`DSP_RATE`].
pub fn render(s: &Shape) -> Vec<i16> {
    let rate = DSP_RATE as f64;
    let ms = |x: f64| (x * rate / 1000.0).round() as usize;
    let (a, sus, dec) = (ms(s.attack_ms), ms(s.sustain_ms), ms(s.decay_ms));
    let n = (a + sus + dec).max(1);
    let cycle = s
        .repeat
        .map_or(n, |f| ((n as f64 * f).round() as usize).max(1));
    let mut phase = 0.0f64;
    let mut noise = Noise(0x2545_F491); // fixed grain: only the preset's knobs roll
    let mut level = noise.level();
    (0..n)
        .map(|i| {
            let t = (i % cycle) as f64 / cycle as f64;
            let hz = pitch_at(s, t);
            let before = phase;
            phase = (phase + hz / rate).fract();
            let osc = match s.wave {
                Wave::Square => {
                    if phase < s.duty {
                        1.0
                    } else {
                        -1.0
                    }
                }
                Wave::Saw => 2.0 * phase - 1.0,
                Wave::Sine => (std::f64::consts::TAU * phase).sin(),
                Wave::Noise => {
                    if (phase * NOISE_STEPS).floor() != (before * NOISE_STEPS).floor() {
                        level = noise.level();
                    }
                    level
                }
            };
            let env = if i < a {
                i as f64 / a as f64
            } else if i < a + sus {
                1.0
            } else {
                // Squared fall: sounds like a decay, not a fade.
                let left = (n - i) as f64 / dec.max(1) as f64;
                left * left
            };
            (osc * env * 0.6 * 32767.0).round() as i16
        })
        .collect()
}

/// Pitch at `t`, the position in the motion cycle (0..1).
fn pitch_at(s: &Shape, t: f64) -> f64 {
    let mut st = s.sweep_st * t;
    if let Some(floor) = s.floor_st {
        st = st.max(floor);
    }
    if let Some((at, jump)) = s.arp {
        if t >= at {
            st += jump;
        }
    }
    (s.pitch_hz * 2f64.powf(st / 12.0)).clamp(FLOOR_HZ, CEILING_HZ)
}

/// The noise oscillator's levels: a 32-bit xorshift, the same sequence for every seed.
struct Noise(u32);

impl Noise {
    fn level(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0 as f64 / u32::MAX as f64 * 2.0 - 1.0
    }
}

/// Roll, reshape and render: the WAV bytes and what rendered.
pub fn make(preset: Preset, knobs: &Knobs) -> Result<(Vec<u8>, Stats), String> {
    let s = shape(preset, knobs)?;
    let pcm = render(&s);
    let peak = pcm.iter().map(|x| x.unsigned_abs()).max().unwrap_or(0);
    // Glide as played over one cycle: the pitch at its end against the start, without the arp.
    let glide = Shape {
        arp: None,
        ..s.clone()
    };
    let sweep_st = 12.0 * (pitch_at(&glide, 1.0) / pitch_at(&glide, 0.0)).log2();
    let stats = Stats {
        preset,
        seed: knobs.seed.unwrap_or(DEFAULT_SEED),
        wave: s.wave,
        pitch_hz: pitch_at(&glide, 0.0),
        sweep_st,
        attack_ms: s.attack_ms,
        sustain_ms: s.sustain_ms,
        decay_ms: s.decay_ms,
        samples: pcm.len(),
        peak_dbfs: (peak > 0).then(|| 20.0 * (peak as f64 / 32768.0).log10()),
    };
    Ok((crate::wav::write(&pcm, 1, DSP_RATE)?, stats))
}

/// A finite decimal, after an optional unit word (any case) is taken off its end.
fn decimal(raw: &str, unit: &str) -> Option<f64> {
    let lower = raw.to_ascii_lowercase();
    let digits = lower.strip_suffix(unit).unwrap_or(&lower);
    digits.parse::<f64>().ok().filter(|v| v.is_finite())
}

/// Milliseconds from `300`, `300ms` or `0.3s`.
fn parse_ms(raw: &str) -> Option<f64> {
    let lower = raw.to_ascii_lowercase();
    match lower.strip_suffix("ms") {
        Some(ms) => decimal(ms, ""),
        None => match lower.strip_suffix('s') {
            Some(seconds) => decimal(seconds, "").map(|v| v * 1000.0),
            None => decimal(&lower, ""),
        },
    }
}

pub const USAGE: &str =
    "usage: ppu sfx <coin|jump|hurt|explosion|powerup|blip|laser> [-o file.wav] \
[--wave square|saw|sine|noise] [--pitch Hz] [--length ms] [--attack ms] [--decay ms] \
[--sweep semitones] [--seed N]";

/// Parse `ppu sfx`'s arguments (after `sfx`). Every problem is reported at once; nothing is
/// clamped.
pub fn parse(args: &[String]) -> Result<Request, String> {
    let mut errors = Vec::new();
    let mut preset = None;
    let mut knobs = Knobs::default();
    let mut out = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        if !arg.starts_with('-') || arg == "-" {
            if preset.is_some() {
                errors.push(format!("unexpected argument {arg:?}"));
            } else if let Some(&(_, p)) = PRESETS.iter().find(|(n, _)| n == arg) {
                preset = Some(p);
            } else {
                let names: Vec<_> = PRESETS.iter().map(|(n, _)| *n).collect();
                errors.push(format!("unknown preset {arg:?}: {}", names.join(", ")));
            }
            continue;
        }
        let Some(value) = it.next() else {
            errors.push(format!("{arg} requires a value"));
            continue;
        };
        let bad = |what: &str| format!("{arg} {value:?} is not {what}");
        match arg.as_str() {
            "-o" | "--out" => out = Some(value.clone()),
            "--wave" => match WAVES.iter().find(|(n, _)| n == value) {
                Some(&(_, w)) => knobs.wave = Some(w),
                None => errors.push(bad("a wave: square, saw, sine or noise")),
            },
            "--seed" => match value.parse::<u64>() {
                Ok(n) => knobs.seed = Some(n),
                Err(_) => errors.push(bad("a whole number 0 or more")),
            },
            "--pitch" => match decimal(value, "hz") {
                Some(v) => {
                    errors.extend(out_of_range("--pitch", "Hz", v, PITCH_HZ));
                    knobs.pitch_hz = Some(v);
                }
                None => errors.push(bad("a pitch in Hz (880 or 880hz)")),
            },
            "--sweep" => match decimal(value, "st") {
                Some(v) => {
                    errors.extend(out_of_range("--sweep", "semitones", v, SWEEP_ST));
                    knobs.sweep_st = Some(v);
                }
                None => errors.push(bad("semitones (+12, -7 or 12st)")),
            },
            "--length" | "--attack" | "--decay" => match parse_ms(value) {
                Some(v) => {
                    let (slot, bounds) = match arg.as_str() {
                        "--length" => (&mut knobs.length_ms, LENGTH_MS),
                        "--attack" => (&mut knobs.attack_ms, STAGE_MS),
                        _ => (&mut knobs.decay_ms, STAGE_MS),
                    };
                    errors.extend(out_of_range(arg, "ms", v, bounds));
                    *slot = Some(v);
                }
                None => errors.push(bad("a time (300, 300ms or 0.3s)")),
            },
            _ => errors.push(format!("unknown flag {arg}")),
        }
    }
    if preset.is_none() && errors.is_empty() {
        errors.push("which preset?".into());
    }
    if !errors.is_empty() {
        errors.push(USAGE.into());
        return Err(errors.join("\n"));
    }
    Ok(Request {
        preset: preset.unwrap(),
        knobs,
        out,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    fn pcm(bytes: &[u8]) -> Vec<i16> {
        crate::wav::read(bytes).unwrap()
    }

    #[test]
    fn every_preset_renders_and_converts_as_a_sample_source() {
        for (name, preset) in PRESETS {
            for seed in [DEFAULT_SEED, 2, 77, u64::MAX] {
                let knobs = Knobs {
                    seed: Some(seed),
                    ..Knobs::default()
                };
                let (bytes, stats) = make(preset, &knobs).unwrap();
                let r = hound::WavReader::new(std::io::Cursor::new(&bytes)).unwrap();
                assert_eq!((r.spec().channels, r.spec().sample_rate), (1, DSP_RATE));
                let samples = pcm(&bytes);
                assert_eq!(samples.len(), stats.samples, "{name}");
                assert!(samples.len() <= MAX_MS as usize * 32, "{name} {seed}");
                assert!(stats.peak_dbfs.is_some(), "{name} {seed} is silent");
                crate::convert_sample(&samples, &crate::ConvertSampleOptions::default())
                    .unwrap_or_else(|e| panic!("{name} seed {seed}: {e}"));
            }
        }
    }

    #[test]
    fn the_longest_sound_still_converts() {
        let knobs = Knobs {
            length_ms: Some(MAX_MS as f64),
            ..Knobs::default()
        };
        let (bytes, _) = make(Preset::Explosion, &knobs).unwrap();
        crate::convert_sample(&pcm(&bytes), &crate::ConvertSampleOptions::default()).unwrap();
    }

    #[test]
    fn same_request_same_bytes_and_a_seed_rolls_a_variation() {
        for (_, preset) in PRESETS {
            let a = make(preset, &Knobs::default()).unwrap().0;
            assert_eq!(a, make(preset, &Knobs::default()).unwrap().0);
            let other = Knobs {
                seed: Some(1234),
                ..Knobs::default()
            };
            assert_ne!(a, make(preset, &other).unwrap().0);
        }
    }

    #[test]
    fn length_alone_scales_the_shape() {
        let s = roll(Preset::Coin, DEFAULT_SEED);
        let knobs = Knobs {
            length_ms: Some(s.length_ms() * 2.0),
            ..Knobs::default()
        };
        let t = shape(Preset::Coin, &knobs).unwrap();
        assert!((t.sustain_ms - 2.0 * s.sustain_ms).abs() < 1e-9);
        assert!((t.decay_ms - 2.0 * s.decay_ms).abs() < 1e-9);
        assert_eq!(
            t.arp, s.arp,
            "the arp is a fraction of the sound, so it stretches along"
        );
    }

    #[test]
    fn pinned_stages_are_exact() {
        let s = roll(Preset::Jump, DEFAULT_SEED);
        let t = shape(
            Preset::Jump,
            &Knobs {
                attack_ms: Some(10.0),
                decay_ms: Some(500.0),
                ..Knobs::default()
            },
        )
        .unwrap();
        assert_eq!(
            (t.attack_ms, t.sustain_ms, t.decay_ms),
            (10.0, s.sustain_ms, 500.0)
        );
        // With a length, the unpinned stages share what is left in the preset's proportions.
        let t = shape(
            Preset::Jump,
            &Knobs {
                length_ms: Some(400.0),
                attack_ms: Some(100.0),
                ..Knobs::default()
            },
        )
        .unwrap();
        assert_eq!(t.attack_ms, 100.0);
        assert!((t.length_ms() - 400.0).abs() < 1e-9);
        assert!((t.sustain_ms / t.decay_ms - s.sustain_ms / s.decay_ms).abs() < 1e-9);
        let (bytes, stats) = make(
            Preset::Jump,
            &Knobs {
                length_ms: Some(400.0),
                ..Knobs::default()
            },
        )
        .unwrap();
        assert_eq!(pcm(&bytes).len(), 12_800);
        assert_eq!(stats.samples, 12_800);
    }

    #[test]
    fn conflicts_and_bounds_are_refused_naming_them() {
        let e = shape(
            Preset::Coin,
            &Knobs {
                length_ms: Some(100.0),
                attack_ms: Some(80.0),
                decay_ms: Some(40.0),
                ..Knobs::default()
            },
        )
        .unwrap_err();
        assert!(
            e.contains("80") && e.contains("40") && e.contains("100"),
            "{e}"
        );
        let e = shape(
            Preset::Coin,
            &Knobs {
                attack_ms: Some(3000.0),
                decay_ms: Some(3000.0),
                ..Knobs::default()
            },
        )
        .unwrap_err();
        assert!(e.contains(&MAX_MS.to_string()), "{e}");

        let e = parse(&args(
            "coin --pitch 9000 --length 5s --sweep 80st --wave triangle",
        ))
        .unwrap_err();
        assert!(
            e.contains("--pitch 9000 Hz is out of range: 30 to 8000"),
            "{e}"
        );
        assert!(
            e.contains(&format!("--length 5000 ms is out of range: 10 to {MAX_MS}")),
            "{e}"
        );
        assert!(
            e.contains("--sweep 80 semitones is out of range: -72 to 72"),
            "{e}"
        );
        assert!(e.contains("--wave \"triangle\""), "{e}");
        let e = parse(&args("coin --seed -3 --decay soon")).unwrap_err();
        assert!(
            e.contains("--seed") && e.contains("--decay \"soon\""),
            "{e}"
        );
        assert!(parse(&args("bell")).unwrap_err().contains("unknown preset"));
        assert!(parse(&args("coin --pitch"))
            .unwrap_err()
            .contains("requires a value"));
        assert!(parse(&[]).is_err());
    }

    #[test]
    fn flags_speak_real_units() {
        let r = parse(&args(
            "laser -o zap.wav --wave saw --pitch 880hz --length 0.3s --attack 5ms --decay 100 --sweep -12st --seed 4",
        ))
        .unwrap();
        assert_eq!(r.preset, Preset::Laser);
        assert_eq!(r.out.as_deref(), Some("zap.wav"));
        assert_eq!(
            r.knobs,
            Knobs {
                wave: Some(Wave::Saw),
                pitch_hz: Some(880.0),
                length_ms: Some(300.0),
                attack_ms: Some(5.0),
                decay_ms: Some(100.0),
                sweep_st: Some(-12.0),
                seed: Some(4),
            }
        );
        let (_, stats) = make(r.preset, &r.knobs).unwrap();
        assert_eq!(stats.pitch_hz, 880.0);
        assert!(
            (stats.sweep_st + 12.0).abs() < 1e-9,
            "--sweep drops the laser's floor"
        );
        assert_eq!(stats.samples, 9600);
    }

    #[test]
    fn a_floor_or_the_ceiling_shows_in_the_stats() {
        let (_, stats) = make(Preset::Laser, &Knobs::default()).unwrap();
        let s = roll(Preset::Laser, DEFAULT_SEED);
        assert!((stats.sweep_st - s.floor_st.unwrap()).abs() < 1e-9);
        let knobs = Knobs {
            pitch_hz: Some(4000.0),
            sweep_st: Some(24.0),
            ..Knobs::default()
        };
        let (_, stats) = make(Preset::Jump, &knobs).unwrap();
        assert!((stats.sweep_st - 12.0).abs() < 1e-9, "{}", stats.sweep_st);
    }
}
