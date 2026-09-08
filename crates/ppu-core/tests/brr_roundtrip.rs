//! PPU-136 :: `import::brr::{encode_brr, decode_brr}` round-trip evidence.
//! Only the public seam is touched here — never `dsp::brr` directly.

use ppu_core::import::brr::{decode_brr, encode_brr};

/// `amplitude * sin(2*pi*i/period)`, rounded to the nearest i16.
fn sine(len: usize, period: f64, amplitude: f64) -> Vec<i16> {
    (0..len)
        .map(|i| {
            (amplitude * (2.0 * std::f64::consts::PI * i as f64 / period).sin()).round() as i16
        })
        .collect()
}

#[test]
fn sine_no_loop_meets_snr_floor_and_reports_match_decode() {
    // 1 kHz at 32 kHz -> 32 samples/period; 3200 samples = 100 periods = 200
    // 16-sample blocks exactly (no padding).
    let pcm = sine(3200, 32.0, 20000.0);
    let (brr, report) = encode_brr(&pcm, None);

    assert_eq!(report.blocks, 200);
    assert_eq!(report.bytes, 1800);
    assert_eq!(brr.len(), 1800);

    let last_header = brr[brr.len() - 9];
    assert_eq!(last_header & 0x01, 0x01, "last block must carry END");
    assert_eq!(last_header & 0x02, 0x00, "no loop requested -> LOOP clear");

    assert!(
        report.snr_db >= 30.0,
        "snr_db {} below the 30 dB floor",
        report.snr_db
    );

    // Independent recomputation straight from the public decoder, matching
    // the report's own definition of SNR bit for bit.
    let decoded = decode_brr(&brr, None, 3200);
    let mut sum_sig: i64 = 0;
    let mut sum_err: i64 = 0;
    for i in 0..3200 {
        let x = pcm[i] as i64;
        let y = decoded[i] as i64;
        sum_sig += x * x;
        let d = x - y;
        sum_err += d * d;
    }
    let expected_snr = if sum_err == 0 {
        120.0
    } else {
        10.0 * (sum_sig as f64 / sum_err as f64).log10()
    };
    assert!(
        (report.snr_db - expected_snr).abs() < 1e-9,
        "report snr_db {} vs recomputed {}",
        report.snr_db,
        expected_snr
    );

    assert!(!report.loop_moved);
    assert_eq!(report.loop_start, None);
}

#[test]
fn looped_sine_reenters_seamlessly_across_the_loop_boundary() {
    let pcm = sine(640, 32.0, 20000.0);
    let (brr, _report) = encode_brr(&pcm, Some(96)); // already block-aligned (block 6)

    let d = decode_brr(&brr, Some(6), 1280);

    // Non-vacuity: the decoded stream carries the 20000-amplitude sine in
    // BOTH passes (a silent decode would satisfy the equalities below).
    let peak = |s: &[i16]| s.iter().map(|v| (*v as i32).abs()).max().unwrap();
    assert!(
        peak(&d[..640]) >= 15000,
        "first pass peak {}",
        peak(&d[..640])
    );
    assert!(
        peak(&d[640..]) >= 15000,
        "loop pass peak {}",
        peak(&d[640..])
    );

    // Second pass over the loop body is bit-identical to the first.
    assert_eq!(&d[640..1184], &d[96..640]);

    // The step across the loop boundary is no worse than the largest
    // adjacent step already present in the first pass.
    let max_step_inside = (0..639)
        .map(|i| (d[i + 1] as i32 - d[i] as i32).abs())
        .max()
        .unwrap();
    let boundary_step = (d[640] as i32 - d[639] as i32).abs();
    assert!(
        boundary_step <= max_step_inside,
        "boundary step {} exceeds max interior step {}",
        boundary_step,
        max_step_inside
    );
}

#[test]
fn loop_start_aligns_down_to_a_block_and_reports_the_move() {
    let pcm = sine(640, 32.0, 20000.0);
    let (brr, report) = encode_brr(&pcm, Some(100)); // 100 & !15 == 96

    assert_eq!(report.loop_start, Some(96));
    assert!(report.loop_moved);

    let last_header = brr[brr.len() - 9];
    assert_eq!(last_header & 0x02, 0x02, "aligned loop -> LOOP bit set");
}

#[test]
fn short_sample_pads_to_a_block_and_decodes_silence_after_end() {
    let pcm: Vec<i16> = (0..20).map(|i| (i as i16) * 100).collect();
    let (brr, report) = encode_brr(&pcm, None);

    assert_eq!(report.blocks, 2);
    assert_eq!(report.bytes, 18);
    assert_eq!(brr.len(), 18);

    let d = decode_brr(&brr, None, 32);
    for (i, &s) in d.iter().enumerate().take(32).skip(20) {
        assert!(
            s.unsigned_abs() <= 64,
            "sample {i} = {s} not close enough to silence"
        );
    }
}
