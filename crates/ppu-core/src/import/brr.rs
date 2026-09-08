//! BRR (Bit Rate Reduction) sample encoder/decoder helper for the importer.
//!
//! `encode_brr` picks, per 16-sample block, the prediction filter (0..=3)
//! and shift (0..=12) that the reference decoder (`crate::dsp::brr::
//! BrrBlockDecoder`) reproduces most accurately, reusing that same decoder
//! both to judge candidates and to report quality — no second decoder.
//! `decode_brr` is a small hardware-semantics wrapper over it (history
//! carries across blocks, optional loop-block continuation) for tests and
//! tooling that want to hear the encoder's own output.

use crate::dsp::brr::{decode_sample, BrrBlockDecoder};

/// Quality/shape report for one `encode_brr` call. See field docs for exact
/// semantics; `snr_db` and `max_error` are computed against the DSP core's
/// own decoder output, not the encoder's internal bookkeeping.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct SampleReport {
    /// Input PCM frames (before zero-padding to a block boundary).
    pub samples: usize,
    /// 9-byte BRR blocks emitted.
    pub blocks: usize,
    /// `blocks * 9`.
    pub bytes: usize,
    /// `10*log10(sum x^2 / sum (x-y)^2)` vs `decode_brr(&brr, None, samples)`
    /// over the unpadded input; `120.0` when the error is exactly zero.
    pub snr_db: f64,
    /// Max `|x - y|` over the unpadded samples.
    pub max_error: u16,
    /// Block-aligned loop point actually encoded, in samples.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loop_start: Option<usize>,
    /// True when the requested `loop_start` was not already block-aligned.
    pub loop_moved: bool,
}

/// Choose the best nibble for each of a block's 16 target samples,
/// sequentially, seeding the running history and rolling it forward from
/// each choice's own decoded output (the candidate's own prediction basis).
/// Scores each candidate nibble with `dsp::brr::decode_sample` — the same
/// per-sample step `BrrBlockDecoder::read` uses — so the returned `decoded`
/// samples and `sse` are the real decoder's output, not a re-decode of a
/// packed block.
fn quantize_candidate(
    filter: u8,
    shift: u8,
    target: &[i32; 16],
    seed_last: i16,
    seed_last_last: i16,
) -> ([i32; 16], [i16; 16], i64) {
    let mut last = seed_last;
    let mut last_last = seed_last_last;
    let mut nibbles = [0i32; 16];
    let mut decoded = [0i16; 16];
    let mut sse: i64 = 0;
    for i in 0..16 {
        let mut best_n = -8i32;
        let mut best_dec = 0i16;
        let mut best_err: i64 = i64::MAX;
        for n in -8..=7i32 {
            let dec = decode_sample(n, shift, filter, last, last_last);
            let err = dec as i64 - target[i] as i64;
            let err2 = err * err;
            if err2 < best_err {
                best_err = err2;
                best_n = n;
                best_dec = dec;
            }
        }
        nibbles[i] = best_n;
        decoded[i] = best_dec;
        sse += best_err;
        last_last = last;
        last = best_dec;
    }
    (nibbles, decoded, sse)
}

/// One scored candidate: (filter, shift, sum-of-squared-error, nibbles, decoded samples).
type BlockCandidate = (u8, u8, i64, [i32; 16], [i16; 16]);

/// Pack a header byte and 16 4-bit nibbles into a 9-byte BRR block.
fn pack_block(header: u8, nibbles: &[i32; 16]) -> [u8; 9] {
    let mut b = [0u8; 9];
    b[0] = header;
    for g in 0..4 {
        let n0 = (nibbles[4 * g] & 0x0f) as u8;
        let n1 = (nibbles[4 * g + 1] & 0x0f) as u8;
        let n2 = (nibbles[4 * g + 2] & 0x0f) as u8;
        let n3 = (nibbles[4 * g + 3] & 0x0f) as u8;
        b[1 + 2 * g] = (n0 << 4) | n1;
        b[2 + 2 * g] = (n2 << 4) | n3;
    }
    b
}

/// Encode `pcm` to BRR, trying every filter (block 0 and the loop-start
/// block restricted to filter 0, which is history-independent) and every
/// shift per 16-sample block, judged by `BrrBlockDecoder`'s own output.
/// Zero-padded to a block boundary; empty `pcm` yields one silent END block.
/// `loop_start` aligns down to a block boundary; `loop_start >= pcm.len()`
/// is a caller error (validated by `debug_assert!`, not by this function).
pub fn encode_brr(pcm: &[i16], loop_start: Option<usize>) -> (Vec<u8>, SampleReport) {
    let samples = pcm.len();
    let padded_len = if pcm.is_empty() {
        16
    } else {
        pcm.len().div_ceil(16) * 16
    };
    let num_blocks = padded_len / 16;

    let mut padded: Vec<i32> = pcm.iter().map(|&s| s as i32).collect();
    padded.resize(padded_len, 0);

    let aligned_loop = loop_start.map(|s| {
        debug_assert!(s < pcm.len(), "loop_start out of range");
        s & !15
    });
    let loop_moved = matches!((loop_start, aligned_loop), (Some(r), Some(a)) if r != a);
    let loop_block = aligned_loop.map(|a| a / 16);

    let mut brr = Vec::with_capacity(num_blocks * 9);
    let mut last: i16 = 0;
    let mut last_last: i16 = 0;

    for block in 0..num_blocks {
        let is_last = block == num_blocks - 1;
        let is_loop_block = loop_block == Some(block);
        let filters: &[u8] = if block == 0 || is_loop_block {
            &[0]
        } else {
            &[0, 1, 2, 3]
        };
        let mut target = [0i32; 16];
        target.copy_from_slice(&padded[block * 16..block * 16 + 16]);

        let mut best: Option<BlockCandidate> = None;
        for &filter in filters {
            for shift in 0..=12u8 {
                let (nibbles, decoded, sse) =
                    quantize_candidate(filter, shift, &target, last, last_last);
                let better = match &best {
                    None => true,
                    Some((_, _, best_sse, _, _)) => sse < *best_sse,
                };
                if better {
                    best = Some((filter, shift, sse, nibbles, decoded));
                }
            }
        }
        let (filter, shift, _sse, nibbles, decoded) = best.unwrap();

        let end = is_last;
        let loop_flag = is_last && loop_start.is_some();
        let header = (shift << 4) | (filter << 2) | ((loop_flag as u8) << 1) | (end as u8);
        brr.extend_from_slice(&pack_block(header, &nibbles));

        last_last = decoded[14];
        last = decoded[15];
    }

    let decoded = decode_brr(&brr, None, samples);
    let mut sum_sig: i64 = 0;
    let mut sum_err: i64 = 0;
    let mut max_error: i64 = 0;
    for i in 0..samples {
        let x = pcm[i] as i64;
        let y = decoded[i] as i64;
        sum_sig += x * x;
        let d = x - y;
        sum_err += d * d;
        max_error = max_error.max(d.abs());
    }
    let snr_db = if sum_err == 0 {
        120.0
    } else {
        10.0 * (sum_sig as f64 / sum_err as f64).log10()
    };

    let report = SampleReport {
        samples,
        blocks: num_blocks,
        bytes: brr.len(),
        snr_db,
        max_error: max_error as u16,
        loop_start: aligned_loop,
        loop_moved,
    };
    (brr, report)
}

/// Decode `samples` output samples with hardware semantics: history (last
/// two decoded samples) carries across blocks with no reset; after a block
/// with the END flag, continue at `loop_block` when Some (history still
/// carries), else emit zeros for the remainder.
pub fn decode_brr(brr: &[u8], loop_block: Option<usize>, samples: usize) -> Vec<i16> {
    let mut out = Vec::with_capacity(samples);
    let total_blocks = brr.len() / 9;
    if total_blocks == 0 {
        out.resize(samples, 0);
        return out;
    }

    let mut dec = BrrBlockDecoder::new();
    let mut block_idx = 0usize;
    dec.read(&brr[0..9]);
    let mut silent = false;

    while out.len() < samples {
        if silent {
            out.push(0);
            continue;
        }
        if dec.is_finished() {
            if dec.is_end {
                match loop_block {
                    Some(lb) if lb < total_blocks => {
                        block_idx = lb;
                        dec.read(&brr[block_idx * 9..block_idx * 9 + 9]);
                    }
                    _ => {
                        silent = true;
                        continue;
                    }
                }
            } else {
                block_idx += 1;
                if block_idx >= total_blocks {
                    silent = true;
                    continue;
                }
                dec.read(&brr[block_idx * 9..block_idx * 9 + 9]);
            }
        }
        out.push(dec.read_next_sample());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `quantize_candidate` now calls `dsp::brr::decode_sample` directly
    /// instead of packing each candidate and re-decoding it through
    /// `BrrBlockDecoder`. Pins `encode_brr`'s output for a fixed ramp to the
    /// bytes captured from the pre-refactor implementation, proving the
    /// change is byte-identical.
    #[test]
    fn encode_brr_ramp_bytes_unchanged_by_the_decode_sample_refactor() {
        let pcm: Vec<i16> = (0..64).map(|i| (i as i16) * 500 - 16000).collect();
        let (bytes, _report) = encode_brr(&pcm, None);
        const EXPECTED: [u8; 36] = [
            176, 136, 153, 153, 170, 170, 187, 187, 204, 88, 226, 170, 187, 189, 206, 222, 255,
            241, 92, 35, 67, 67, 84, 85, 86, 87, 87, 109, 52, 52, 68, 68, 69, 69, 85, 85,
        ];
        assert_eq!(bytes, EXPECTED);
    }
}
