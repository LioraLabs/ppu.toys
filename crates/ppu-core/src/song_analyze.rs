//! A compiled song's per-step voice load: what the panel's voice strip shows.
//!
//! A step is each 16th (12 units) of each arrangement slot, the Grid's
//! resolution: a slot of `len` units has `ceil(len / 12)` steps, starting at
//! the slot's position + 12k. A step's tick is `Timing::tick` of its start.

use crate::song::{compile, Event, Song, SongError, UNITS_PER_BEAT};

const UNITS_PER_16TH: u64 = UNITS_PER_BEAT as u64 / 4;

/// The most steps `analyze` lays out, so a hostile pattern length can't make
/// it allocate unbounded step arrays (a million 16ths is over 4 hours at
/// 400 BPM).
const MAX_STEPS: u64 = 1_000_000;

/// A song's compiled events plus its per-step voice counts.
#[derive(Debug)]
pub struct Analysis {
    /// Start-ordered, voice-assigned (the same events `score{}` plays).
    pub events: Vec<Event>,
    /// Song length, in engine ticks.
    pub length: i64,
    /// Index of each arrangement slot's first step.
    pub slot_steps: Vec<u32>,
    /// Per step: voices sounding at its tick, after steals and pin cuts.
    pub used: Vec<u8>,
    /// Per step: notes whose uncut span covers its tick.
    pub wanted: Vec<u32>,
    /// Steps where `wanted` exceeds the voices in the song's mask.
    pub over: Vec<u32>,
}

/// Compiles `song` (same errors as `score{}` setup) and counts voices per
/// step in one pass over the events: each event adds +1/-1 at the steps its
/// span starts and ends (binary search into the step ticks), then a prefix
/// sum. No steps x events scan.
pub fn analyze(song: &Song) -> Result<Analysis, SongError> {
    let c = compile(song)?;
    let t = &c.timing;
    let total: u64 = t
        .slots
        .iter()
        .map(|s| (s.len as u64).div_ceil(UNITS_PER_16TH))
        .sum();
    if total > MAX_STEPS {
        return Err(SongError(format!(
            "the song is over {MAX_STEPS} sixteenths long"
        )));
    }

    let mut ticks = Vec::with_capacity(total as usize);
    let mut slot_steps = Vec::with_capacity(t.slots.len());
    for s in &t.slots {
        slot_steps.push(ticks.len() as u32);
        ticks.extend(
            (0..s.len as u64)
                .step_by(UNITS_PER_16TH as usize)
                .map(|u| t.tick(s.pos + u)),
        );
    }

    // Step range [first tick >= from, first tick >= to): the steps whose
    // tick lies in [from, to).
    let at = |tick: i64| ticks.partition_point(|&x| x < tick);
    let n = ticks.len();
    let mut used_d = vec![0i64; n + 1];
    let mut wanted_d = vec![0i64; n + 1];
    for e in &c.events {
        let first = at(e.start);
        used_d[first] += 1;
        used_d[at(e.end)] -= 1;
        // The note's uncut end, as compile computes it before allocation.
        let slot = &t.slots[e.slot as usize];
        let note = &song.patterns[slot.pattern as usize].notes[e.note as usize];
        let end_pos = (slot.pos + note.at as u64 + note.len as u64).min(t.end);
        wanted_d[first] += 1;
        wanted_d[at(t.tick(end_pos).max(e.start))] -= 1;
    }

    let voices = song.voice_mask.count_ones();
    let (mut u, mut w) = (0i64, 0i64);
    let mut used = Vec::with_capacity(n);
    let mut wanted = Vec::with_capacity(n);
    let mut over = Vec::new();
    for i in 0..n {
        u += used_d[i];
        w += wanted_d[i];
        used.push(u as u8);
        wanted.push(w as u32);
        if w as u32 > voices {
            over.push(i as u32);
        }
    }

    Ok(Analysis {
        length: t.length,
        events: c.events,
        slot_steps,
        used,
        wanted,
        over,
    })
}
