//! A compiled song's per-step voice load: what the panel's voice strip shows.
//!
//! A step is each 16th (12 units) of each arrangement slot, the Grid's
//! resolution: a slot of `len` units has `ceil(len / 12)` steps, starting at
//! the slot's position + 12k (`Timing::steps` / `Timing::step_tick`, the same
//! steps `Timing::position` maps a tick back to).

use crate::song::{compile, Event, Song, SongError};

/// The most steps `analyze` lays out, so a hostile pattern length can't make
/// it allocate unbounded step arrays (a million 16ths is over 4 hours at
/// 400 BPM).
const MAX_STEPS: u64 = 1_000_000;

/// A song's compiled events plus its per-step voice counts. Step `i` spans
/// the ticks from its own tick up to the next step's (the last step runs on
/// past the song's end, keeping any note nudged late).
#[derive(Debug, PartialEq)]
pub struct Analysis {
    /// Start-ordered, voice-assigned (the same events `score{}` plays).
    pub events: Vec<Event>,
    /// Song length, in engine ticks.
    pub length: i64,
    /// The tick the song loops back to (the loop-start slot's first step).
    pub loop_tick: i64,
    /// Index of each arrangement slot's first step.
    pub slot_steps: Vec<u32>,
    /// Per step: the most voices sounding at once in it, after steals and
    /// pin cuts.
    pub used: Vec<u8>,
    /// Per step: the most notes sounding at once in it, uncut.
    pub wanted: Vec<u32>,
    /// Steps where `wanted` exceeds the voices in the song's mask.
    pub over: Vec<u32>,
}

/// Compiles `song` (same errors as `score{}` setup) and counts voices per
/// step with one sweep over the sorted note edges. No steps x events scan.
pub fn analyze(song: &Song) -> Result<Analysis, SongError> {
    let c = compile(song)?;
    let t = &c.timing;
    let total: u64 = (0..t.slots.len()).map(|s| t.steps(s)).sum();
    if total > MAX_STEPS {
        return Err(SongError(format!(
            "the song is over {MAX_STEPS} sixteenths long"
        )));
    }

    let mut ticks = Vec::with_capacity(total as usize);
    let mut slot_steps = Vec::with_capacity(t.slots.len());
    for s in 0..t.slots.len() {
        slot_steps.push(ticks.len() as u32);
        ticks.extend((0..t.steps(s)).map(|k| t.step_tick(s, k)));
    }

    let used = peaks(c.events.iter().map(|e| (e.start, e.end)), &ticks);
    let wanted = peaks(c.events.iter().map(|e| (e.start, e.want_end)), &ticks);
    let voices = song.voice_mask.count_ones();
    let over = (0..wanted.len() as u32)
        .filter(|&i| wanted[i as usize] > voices)
        .collect();

    Ok(Analysis {
        length: t.length,
        loop_tick: t.loop_tick,
        events: c.events,
        slot_steps,
        used: used.into_iter().map(|u| u as u8).collect(),
        wanted,
        over,
    })
}

/// Per step (starting at each of `ticks`, ascending), the most `[start,
/// end)` spans sounding at once at any tick in it. Ends sort before starts
/// at the same tick, so back-to-back notes never overlap and a zero-length
/// (fully cut) span never counts.
fn peaks(spans: impl Iterator<Item = (i64, i64)>, ticks: &[i64]) -> Vec<u32> {
    let mut edges: Vec<(i64, i32)> = spans.flat_map(|(s, e)| [(s, 1), (e, -1)]).collect();
    edges.sort_unstable();
    let (mut cur, mut j) = (0i32, 0);
    let mut out = Vec::with_capacity(ticks.len());
    for (i, &at) in ticks.iter().enumerate() {
        while j < edges.len() && edges[j].0 <= at {
            cur += edges[j].1;
            j += 1;
        }
        let next = ticks.get(i + 1).copied().unwrap_or(i64::MAX);
        // A step squeezed to no ticks (a fast tempo) holds nothing.
        let mut peak = if next > at { cur } else { 0 };
        while j < edges.len() && edges[j].0 < next {
            cur += edges[j].1;
            j += 1;
            peak = peak.max(cur);
        }
        out.push(peak as u32);
    }
    out
}
