//! Built-in sample bank: `dma("piano")` works in every toy with no upload.
//!
//! Every sample is synthesized here (deterministically, no assets) as 32 kHz
//! mono PCM and BRR-encoded on first use, once per process. A user source
//! registered under the same name shadows the built-in — both lookup sites
//! in `lua.rs` try the store first and fall back to [`get`].
//!
//! SNES-appropriate means small: melodic instruments are a 368-sample loop
//! (3 cycles = 260.87 Hz, 5 cents under C4, so presets use `base = "C4"`)
//! behind an optional short attack, ~200-1000 bytes each. Drums are
//! one-shots rendered at 16 kHz and played back at pitch `0x0800`, the way
//! period games saved sound RAM; the kit's `bank()` presets carry that
//! pitch. The whole bank is ~20 KB.

use std::collections::HashMap;
use std::f64::consts::TAU;
use std::sync::{Mutex, OnceLock};

use crate::source::{convert_sample, ConvertSampleOptions, SourcePayload};

/// One loop of the melodic waveforms, in samples. 16-aligned for BRR.
const LOOP: usize = 368;
/// Cycles of the fundamental per loop.
const CYCLES: f64 = 3.0;
/// Drum render rate; presets play drums at `0x0800` to hear this rate.
const DRUM_RATE: f64 = 16000.0;

pub const NAMES: [&str; 15] = [
    "piano", "bass", "lead", "strings", "organ", "bell", "flute", "pluck", "kick", "snare", "hat",
    "ohat", "tom", "clap", "crash",
];

/// The built-in payload for `name`, encoded on first request.
pub fn get(name: &str) -> Option<SourcePayload> {
    if !NAMES.contains(&name) {
        return None;
    }
    static CACHE: OnceLock<Mutex<HashMap<&'static str, SourcePayload>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut cache = cache.lock().unwrap();
    let key = NAMES.iter().find(|n| **n == name).copied()?;
    Some(
        cache
            .entry(key)
            .or_insert_with(|| {
                let (pcm, loop_start) = render(key);
                convert_sample(&pcm, &ConvertSampleOptions { loop_start })
                    .expect("built-in sample encodes")
                    .0
            })
            .clone(),
    )
}

/// `(pcm, loop_start)` for a built-in name.
fn render(name: &str) -> (Vec<i16>, Option<usize>) {
    match name {
        // ---- melodic: (attack loops, bright partials) -> (loop partials) --
        // Partials are (cycles per loop, amplitude); 3 = the fundamental.
        "piano" => looped(
            2,
            &harm(&[1.0, 0.5, 0.33, 0.25, 0.2, 0.17, 0.14, 0.12]),
            &harm(&[1.0, 0.35, 0.19, 0.12]),
        ),
        "bass" => looped(0, &[], &[(3.0, 1.0), (6.0, 0.3), (9.0, 0.33), (15.0, 0.2)]),
        "lead" => looped(0, &[], &pulse(0.25, 12)),
        "strings" => looped(
            0,
            &[],
            &harm(&[1.0, 0.5, 0.33, 0.25, 0.2, 0.17, 0.14, 0.12, 0.11, 0.1]),
        ),
        "organ" => looped(
            0,
            &[],
            &[
                (3.0, 1.0),
                (6.0, 0.8),
                (9.0, 0.5),
                (12.0, 0.6),
                (18.0, 0.4),
                (24.0, 0.3),
            ],
        ),
        "bell" => looped(
            2,
            &[(3.0, 1.0), (6.0, 0.6), (8.0, 0.5), (12.0, 0.3), (16.0, 0.2)],
            &[(3.0, 1.0), (6.0, 0.3)],
        ),
        "flute" => {
            let (mut pcm, ls) = looped(1, &harm(&[1.0, 0.15, 0.1]), &harm(&[1.0, 0.15, 0.1]));
            let mut rng = Lcg(7);
            for (i, s) in pcm.iter_mut().take(LOOP).enumerate() {
                let breath = rng.next() * 0.25 * (1.0 - i as f64 / LOOP as f64);
                *s = (*s as f64 * 0.75 + breath * 32767.0) as i16;
            }
            (pcm, ls)
        }
        "pluck" => looped(
            4,
            &harm(&(1..=16).map(|n| 1.0 / n as f64).collect::<Vec<_>>()),
            &harm(&[1.0, 0.25, 0.11]),
        ),
        // ---- drums: one-shots at DRUM_RATE ------------------------------
        "kick" => drum(0.3, |t, rng| {
            // 160 Hz sweeping down to 45 Hz: phase is the integral of that.
            let ph = TAU * (45.0 * t + 115.0 * 0.05 * (1.0 - (-t / 0.05).exp()));
            let click = if t < 0.003 { rng.next() * 0.5 } else { 0.0 };
            ph.sin() * (-t / 0.1).exp() + click
        }),
        "snare" => drum(0.2, |t, rng| {
            (TAU * 180.0 * t).sin() * 0.5 * (-t / 0.05).exp() + rng.next() * 0.8 * (-t / 0.07).exp()
        }),
        // Pure first-difference noise alternates sign every sample and all
        // but vanishes under half-rate Gaussian interpolation, so the hats
        // mix plain noise in and add a little metallic ring.
        "hat" => drum(0.07, |t, rng| {
            let ring = (TAU * 5200.0 * t).sin() * 0.2 + (TAU * 6900.0 * t).sin() * 0.15;
            (rng.next() * 0.6 + rng.hp() * 0.6 + ring) * (-t / 0.02).exp()
        }),
        "ohat" => drum(0.3, |t, rng| {
            let ring = (TAU * 5200.0 * t).sin() * 0.15 + (TAU * 6900.0 * t).sin() * 0.1;
            (rng.next() * 0.6 + rng.hp() * 0.6 + ring) * (-t / 0.1).exp()
        }),
        "tom" => drum(0.3, |t, _| {
            let ph = TAU * (110.0 * t + 90.0 * 0.08 * (1.0 - (-t / 0.08).exp()));
            ph.sin() * (-t / 0.15).exp()
        }),
        "clap" => drum(0.15, |t, rng| {
            let mut a = 0.0;
            for k in 0..3 {
                let dt = t - k as f64 * 0.01;
                if dt >= 0.0 {
                    a += (-dt / 0.008).exp();
                }
            }
            rng.next() * (a * 0.6 + 0.5 * (-t / 0.05).exp())
        }),
        "crash" => drum(0.5, |t, rng| {
            let metal = (TAU * 3130.0 * t).sin() * 0.15 + (TAU * 4780.0 * t).sin() * 0.1;
            (rng.hp() * 0.8 + metal) * (-t / 0.2).exp()
        }),
        _ => unreachable!("{name} is not a built-in sample"),
    }
}

/// Harmonics 1..n of the fundamental with the given amplitudes.
fn harm(amps: &[f64]) -> Vec<(f64, f64)> {
    amps.iter()
        .enumerate()
        .map(|(i, &a)| (CYCLES * (i + 1) as f64, a))
        .collect()
}

/// Band-limited pulse wave of the given duty cycle, `n` harmonics.
fn pulse(duty: f64, n: usize) -> Vec<(f64, f64)> {
    (1..=n)
        .map(|k| {
            (
                CYCLES * k as f64,
                (k as f64 * std::f64::consts::PI * duty).sin() / k as f64,
            )
        })
        .collect()
}

fn wave(partials: &[(f64, f64)], i: usize) -> f64 {
    let ph = TAU * i as f64 / LOOP as f64;
    partials.iter().map(|&(k, a)| (ph * k).sin() * a).sum()
}

/// `attack_loops` loops crossfading from `bright` to `sustain`, then one
/// loop of `sustain` that loops forever. Both are periodic in LOOP, so the
/// fade and the loop seam are click-free.
fn looped(
    attack_loops: usize,
    bright: &[(f64, f64)],
    sustain: &[(f64, f64)],
) -> (Vec<i16>, Option<usize>) {
    let attack = attack_loops * LOOP;
    let mut pcm: Vec<f64> = (0..attack + LOOP)
        .map(|i| {
            if i < attack {
                let x = i as f64 / attack as f64;
                wave(bright, i) * (1.0 - x) + wave(sustain, i) * x
            } else {
                wave(sustain, i)
            }
        })
        .collect();
    normalize(&mut pcm);
    (to_i16(&pcm), Some(attack))
}

/// A one-shot of `secs` at DRUM_RATE, `f(t, rng)` in roughly -1..1, with a
/// 2 ms fade-out so the END block lands on silence.
fn drum(secs: f64, f: impl Fn(f64, &mut Lcg) -> f64) -> (Vec<i16>, Option<usize>) {
    let n = ((secs * DRUM_RATE) as usize).div_ceil(16) * 16;
    let mut rng = Lcg(0x9e37);
    let mut pcm: Vec<f64> = (0..n)
        .map(|i| {
            let t = i as f64 / DRUM_RATE;
            let fade = ((n - i) as f64 / (0.002 * DRUM_RATE)).min(1.0);
            f(t, &mut rng) * fade
        })
        .collect();
    // Drums are transients played at half rate, where the Gaussian filter
    // already softens them: normalize hotter than the sustained loops.
    normalize_to(&mut pcm, 0.95);
    (to_i16(&pcm), None)
}

/// Peak to 0.8 full scale: the DSP's Gaussian interpolation can overshoot.
fn normalize(pcm: &mut [f64]) {
    normalize_to(pcm, 0.8);
}

fn normalize_to(pcm: &mut [f64], level: f64) {
    let peak = pcm.iter().fold(0.0f64, |m, s| m.max(s.abs()));
    if peak > 0.0 {
        for s in pcm.iter_mut() {
            *s *= level / peak;
        }
    }
}

fn to_i16(pcm: &[f64]) -> Vec<i16> {
    pcm.iter()
        .map(|s| (s * 32767.0).round().clamp(-32768.0, 32767.0) as i16)
        .collect()
}

/// Deterministic noise so the bank is byte-identical everywhere.
struct Lcg(u32);
impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(1664525).wrapping_add(1013904223);
        (self.0 >> 8) as f64 / (1u32 << 24) as f64 * 2.0 - 1.0
    }
    /// First-difference of white noise: a cheap high-pass for cymbals.
    fn hp(&mut self) -> f64 {
        let a = self.next();
        (self.next() - a) * 0.5
    }
}
