//! WAV files in and out. [`read`] turns any WAV (8/16/24/32-bit int or 32-bit float, any channel
//! count, any rate) into the mono 32 kHz PCM [`crate::convert_sample`] takes; [`write`] makes a
//! 16-bit PCM WAV. The folder's WAV is the truth: the resampler need not match a browser's.

use std::io::Cursor;

/// The S-DSP's sample rate, the rate every sample source is converted at.
pub const DSP_RATE: u32 = 32_000;

/// Decode a WAV, mix it to mono and resample it to [`DSP_RATE`], as i16.
pub fn read(bytes: &[u8]) -> Result<Vec<i16>, String> {
    let (mono, rate) = read_mono(bytes)?;
    Ok(resample(&mono, rate, DSP_RATE)
        .into_iter()
        .map(|x| (x * 32768.0).round().clamp(-32768.0, 32767.0) as i16)
        .collect())
}

/// Decode a WAV and mix its channels to mono, in [-1, 1], at the file's own rate.
pub fn read_mono(bytes: &[u8]) -> Result<(Vec<f32>, u32), String> {
    let reader =
        hound::WavReader::new(Cursor::new(bytes)).map_err(|e| format!("not a WAV file: {e}"))?;
    let spec = reader.spec();
    let channels = spec.channels as usize;
    if channels == 0 || spec.sample_rate == 0 {
        return Err("WAV has no channels or no sample rate".into());
    }
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .into_samples::<f32>()
            .collect::<Result<_, _>>()
            .map_err(|e| format!("cannot read WAV samples: {e}"))?,
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1u64 << (spec.bits_per_sample - 1)) as f32;
            reader
                .into_samples::<i32>()
                .map(|s| s.map(|s| s as f32 * scale))
                .collect::<Result<_, _>>()
                .map_err(|e| format!("cannot read WAV samples: {e}"))?
        }
    };
    let mono = samples
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect();
    Ok((mono, spec.sample_rate))
}

/// Windowed-sinc (Blackman) resampling from `from` Hz to `to` Hz. Downsampling lowers the cutoff
/// to the new Nyquist, so nothing above it folds back.
pub fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() {
        return input.to_vec();
    }
    let step = from as f64 / to as f64; // input samples per output sample
    let cutoff = (1.0 / step).min(1.0);
    let half = 16.0 / cutoff; // 16 zero crossings of the cutoff's sinc each side
    let out_len = ((input.len() as f64) / step).round() as usize;
    (0..out_len)
        .map(|n| {
            let t = n as f64 * step;
            let lo = (t - half).ceil().max(0.0) as usize;
            let hi = ((t + half).floor() as usize).min(input.len() - 1);
            let mut sum = 0.0;
            for (k, &x) in input.iter().enumerate().take(hi + 1).skip(lo) {
                let d = t - k as f64;
                let w = 0.42
                    + 0.5 * (std::f64::consts::PI * d / half).cos()
                    + 0.08 * (2.0 * std::f64::consts::PI * d / half).cos();
                sum += x as f64 * cutoff * sinc(cutoff * d) * w;
            }
            sum as f32
        })
        .collect()
}

fn sinc(x: f64) -> f64 {
    if x == 0.0 {
        1.0
    } else {
        let px = std::f64::consts::PI * x;
        px.sin() / px
    }
}

/// A 16-bit PCM WAV of `samples` (interleaved when `channels` is 2) at `rate` Hz.
pub fn write(samples: &[i16], channels: u16, rate: u32) -> Result<Vec<u8>, String> {
    if !(1..=2).contains(&channels) {
        return Err(format!("a WAV is mono or stereo, not {channels} channels"));
    }
    if !samples.len().is_multiple_of(channels as usize) {
        return Err("stereo samples must come in left/right pairs".into());
    }
    let spec = hound::WavSpec {
        channels,
        sample_rate: rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut out = Cursor::new(Vec::new());
    let mut w = hound::WavWriter::new(&mut out, spec).map_err(|e| e.to_string())?;
    for &s in samples {
        w.write_sample(s).map_err(|e| e.to_string())?;
    }
    w.finalize().map_err(|e| e.to_string())?;
    Ok(out.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f64, rate: u32, len: usize, amp: f64) -> Vec<f32> {
        (0..len)
            .map(|i| {
                (amp * (2.0 * std::f64::consts::PI * freq * i as f64 / rate as f64).sin()) as f32
            })
            .collect()
    }

    /// Amplitude of `freq` in `x` (a single-bin DFT), over the middle, away from the edges.
    fn level_at(x: &[f32], freq: f64, rate: u32) -> f64 {
        let mid = &x[x.len() / 4..x.len() * 3 / 4];
        let (mut re, mut im) = (0.0, 0.0);
        for (i, &v) in mid.iter().enumerate() {
            let ph = 2.0 * std::f64::consts::PI * freq * i as f64 / rate as f64;
            re += v as f64 * ph.cos();
            im += v as f64 * ph.sin();
        }
        2.0 * (re * re + im * im).sqrt() / mid.len() as f64
    }

    #[test]
    fn a_resampled_sine_keeps_its_frequency_and_level() {
        for from in [44_100, 22_050, 48_000] {
            let x = sine(1000.0, from, from as usize, 0.5);
            let y = resample(&x, from, DSP_RATE);
            assert_eq!(
                y.len(),
                DSP_RATE as usize,
                "{from} Hz: one second stays one second"
            );
            let at = level_at(&y, 1000.0, DSP_RATE);
            assert!((at - 0.5).abs() < 0.005, "{from} Hz: 1 kHz level {at}");
            let off = level_at(&y, 1100.0, DSP_RATE);
            assert!(off < 0.01, "{from} Hz: energy moved off 1 kHz ({off})");
        }
    }

    #[test]
    fn downsampling_drops_what_lies_above_the_new_nyquist() {
        // 20 kHz cannot exist at 32 kHz; it must not fold back to 12 kHz.
        let y = resample(&sine(20_000.0, 48_000, 48_000, 0.5), 48_000, DSP_RATE);
        let alias = level_at(&y, 12_000.0, DSP_RATE);
        assert!(alias < 0.01, "aliased {alias}");
    }

    #[test]
    fn written_samples_read_back_the_same() {
        let pcm: Vec<i16> = (0..2000)
            .map(|i| ((i * 37) % 65536 - 32768) as i16)
            .collect();
        let bytes = write(&pcm, 1, DSP_RATE).unwrap();
        assert_eq!(read(&bytes).unwrap(), pcm);
        let r = hound::WavReader::new(Cursor::new(&bytes)).unwrap();
        assert_eq!(r.spec().bits_per_sample, 16);
        let back: Vec<i16> = r.into_samples().map(Result::unwrap).collect();
        assert_eq!(back, pcm);

        let stereo = write(&pcm, 2, 44_100).unwrap();
        let r = hound::WavReader::new(Cursor::new(&stereo)).unwrap();
        assert_eq!((r.spec().channels, r.spec().sample_rate), (2, 44_100));
        let back: Vec<i16> = r.into_samples().map(Result::unwrap).collect();
        assert_eq!(back, pcm);
        assert!(write(&pcm[..3], 2, 44_100).is_err());
        assert!(write(&pcm, 3, 44_100).is_err());
    }

    fn wav_with<S: hound::Sample + Copy>(spec: hound::WavSpec, samples: &[S]) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        let mut w = hound::WavWriter::new(&mut out, spec).unwrap();
        for &s in samples {
            w.write_sample(s).unwrap();
        }
        w.finalize().unwrap();
        out.into_inner()
    }

    #[test]
    fn stereo_float_and_odd_widths_mix_to_mono() {
        let spec = |channels, bits, fmt| hound::WavSpec {
            channels,
            sample_rate: DSP_RATE,
            bits_per_sample: bits,
            sample_format: fmt,
        };
        let float = wav_with(spec(1, 32, hound::SampleFormat::Float), &[0.5f32, -0.25]);
        assert_eq!(read(&float).unwrap(), [16384, -8192]);
        // Left and right average.
        let stereo = wav_with(
            spec(2, 16, hound::SampleFormat::Int),
            &[1000i16, 3000, -400, 0],
        );
        assert_eq!(read(&stereo).unwrap(), [2000, -200]);
        let eight = wav_with(spec(1, 8, hound::SampleFormat::Int), &[64i8, -128]);
        assert_eq!(read(&eight).unwrap(), [16384, -32768]);
        let wide = wav_with(
            spec(1, 24, hound::SampleFormat::Int),
            &[0x40_0000i32, -0x80_0000],
        );
        assert_eq!(read(&wide).unwrap(), [16384, -32768]);
        assert!(read(b"\x89PNG not a wav")
            .unwrap_err()
            .contains("not a WAV"));
    }
}
