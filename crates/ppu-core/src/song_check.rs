//! song_check's report: what a composer who cannot hear needs to know about a
//! song. The findings read the compiled song (voice budget, rows above C6,
//! held clashes); the loudness renders it alone on a scratch engine and
//! measures each arrangement slot and each sound. The Studio's song_check tool
//! (through wasm) and `ppu song check` both print [`Report::lines`]. Docs
//! chapter `composing` says how to read and act on it.

use crate::song::{Song, SongError};
use crate::song_analyze::{analyze, Analysis};

const TICKS_PER_SECOND: f64 = 250.0;
const SAMPLES_PER_TICK: usize = 128; // 4 ms at 32 kHz
/// Pitched notes this far apart (mod 12) grate when held: semitone, tritone, major 7th.
const HARSH: [u32; 3] = [1, 6, 11];
/// A clash shorter than this many beats is a passing note.
const CLASH_BEATS: f64 = 0.75;
const MAX_LISTED: usize = 12;
/// A sample is recorded at C4 and the chip's pitch register tops out at four times that: C6.
pub const TOP_NOTE: u32 = 84;

/// What [`check`] found. Levels are dBFS, `None` for silence.
#[derive(Debug, serde::Serialize)]
pub struct Report {
    /// One pass, in seconds.
    pub seconds: f64,
    pub bpm: f64,
    pub slots: usize,
    pub notes: usize,
    pub rows: usize,
    pub voices: Voices,
    /// Rows above C6 that play, as "lead D6".
    pub above_c6: Vec<String>,
    /// Held clashes, advisory.
    pub clashes: Vec<Clash>,
    pub loudness: Loudness,
}

#[derive(Debug, serde::Serialize)]
pub struct Voices {
    /// Most notes wanted at once.
    pub most: u32,
    /// Voices in the song's mask.
    pub budget: u32,
    /// Sixteenths where more notes want to sound than the budget, so one is cut.
    pub over: usize,
    /// Where those sixteenths are, as "chorus beat 3.5", each once.
    pub places: Vec<String>,
    /// The voices kept parts play, which no automatic sound effect takes.
    pub kept: Vec<u8>,
    /// The kept rows' sounds, in row order.
    pub kept_rows: Vec<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct Clash {
    /// "verse beat 2: C#5 against C5, a semitone apart for 1 beats"
    pub text: String,
    /// Times it plays in one pass.
    pub plays: u32,
}

#[derive(Debug, serde::Serialize)]
pub struct Loudness {
    pub slots: Vec<SlotLevel>,
    /// Per sound over the whole song, the others silenced; empty for a one-sound song.
    pub sounds: Vec<SoundLevel>,
    /// Samples at full scale in the mix.
    pub clipped: u32,
}

#[derive(Debug, serde::Serialize)]
pub struct SlotLevel {
    pub pattern: String,
    /// Seconds from the song's start.
    pub at: f64,
    pub rms: Option<f64>,
    pub peak: Option<f64>,
}

#[derive(Debug, serde::Serialize)]
pub struct SoundLevel {
    pub sound: String,
    pub rms: Option<f64>,
    pub peak: Option<f64>,
}

impl Report {
    /// Over the voice budget or above C6: the song does not play as written.
    pub fn failed(&self) -> bool {
        self.voices.over > 0 || !self.above_c6.is_empty()
    }

    /// The report as sentences, `name` heading it.
    pub fn lines(&self, name: &str) -> Vec<String> {
        let mut out = vec![format!(
            "{name}: {:.1} s a pass at {} BPM, {} slots, {} notes, {} rows",
            self.seconds, self.bpm, self.slots, self.notes, self.rows
        )];
        let v = &self.voices;
        out.push(if v.over == 0 {
            format!("voices: at most {} of {} sound at once", v.most, v.budget)
        } else {
            format!(
                "voices: over the {}-voice budget in {} sixteenths, so a note there is cut short: {}",
                v.budget,
                v.over,
                listed(&v.places)
            )
        });
        if !v.kept.is_empty() {
            let voices: Vec<String> = v.kept.iter().map(u8::to_string).collect();
            out.push(format!(
                "kept: voices {} play {}, and no automatic sound effect takes them",
                voices.join(", "),
                v.kept_rows.join(", ")
            ));
        }
        if !self.above_c6.is_empty() {
            out.push(format!(
                "pitch: {} rows are above C6, the chip's top pitch, and play as C6: {}",
                self.above_c6.len(),
                listed(&self.above_c6)
            ));
        }
        if self.clashes.is_empty() {
            out.push("clashes: none held".into());
        } else {
            out.push(format!(
                "clashes (advisory: a held clash can be the point): {}",
                self.clashes.len()
            ));
            for c in self.clashes.iter().take(MAX_LISTED) {
                let plays = if c.plays > 1 {
                    format!(" ({} plays)", c.plays)
                } else {
                    String::new()
                };
                out.push(format!("  {}{plays}", c.text));
            }
            if self.clashes.len() > MAX_LISTED {
                out.push(format!("  and {} more", self.clashes.len() - MAX_LISTED));
            }
        }
        let l = &self.loudness;
        out.push("loudness in dBFS, the song rendered alone with the bank's presets (the toy's BANK edits, echo and mixer are not in it):".into());
        let width = l.slots.iter().map(|s| s.pattern.len()).max().unwrap_or(0);
        for (i, s) in l.slots.iter().enumerate() {
            out.push(format!(
                "  slot {i:>2} {:<width$} {:>6} s  {}",
                s.pattern,
                format!("{:.1}", s.at),
                level(s.rms, s.peak)
            ));
        }
        if !l.sounds.is_empty() {
            out.push("  by sound, over the whole song:".into());
            let width = l.sounds.iter().map(|s| s.sound.len()).max().unwrap_or(0);
            for s in &l.sounds {
                out.push(format!("    {:<width$} {}", s.sound, level(s.rms, s.peak)));
            }
        }
        out.push(if l.clipped > 0 {
            format!(
                "  clipped: {} samples at full scale. Lower the loudest rows' vol, or dsp.mvol.",
                l.clipped
            )
        } else {
            "  clipped: no samples at full scale".into()
        });
        out
    }
}

fn listed(items: &[String]) -> String {
    let mut s = items
        .iter()
        .take(MAX_LISTED)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    if items.len() > MAX_LISTED {
        s += &format!(", and {} more", items.len() - MAX_LISTED);
    }
    s
}

fn db_text(x: Option<f64>) -> String {
    x.map_or("-inf".into(), |d| format!("{d:.1}"))
}

fn level(rms: Option<f64>, peak: Option<f64>) -> String {
    format!("{:>6} rms {:>6} peak", db_text(rms), db_text(peak))
}

/// A number as JavaScript's `+x.toFixed(2)` prints it: at most two decimals, no trailing zeros.
fn short(x: f64) -> String {
    let s = format!("{x:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

const NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];
fn note_name(midi: u32) -> String {
    format!("{}{}", NAMES[(midi % 12) as usize], midi as i32 / 12 - 1)
}

/// Checks `song` (same errors as `score{}` setup). `samples` are the toy's sample sources a row
/// may name, as (source name, encoded source payload).
pub fn check(song: &Song, samples: &[(&str, &[u8])]) -> Result<Report, SongError> {
    let an = analyze(song)?;
    let slots = slot_spans(song, &an);
    let slot_at = |tick: i64| slots.iter().rposition(|s| s.from <= tick).unwrap_or(0);
    let tempo_of = |slot: usize| {
        let p = &song.patterns[song.arrangement[slot] as usize];
        f64::from(p.tempo.unwrap_or(song.tempo))
    };
    let place = |slot: usize, tick: i64| {
        let beat =
            (tick - slots[slot].from) as f64 * tempo_of(slot) / (100.0 * 60.0 * TICKS_PER_SECOND);
        format!(
            "{} beat {}",
            song.patterns[song.arrangement[slot] as usize].name,
            short(beat + 1.0)
        )
    };
    let is_drum = |sound: &str| crate::bank::DRUMS.contains(&sound);

    let mut places: Vec<String> = Vec::new();
    for &step in &an.over {
        let tick = i64::from(an.step_ticks[step as usize]);
        let p = place(slot_at(tick), tick);
        if !places.contains(&p) {
            places.push(p);
        }
    }
    let voices = Voices {
        most: an.wanted.iter().copied().max().unwrap_or(0),
        budget: song.voice_mask.count_ones(),
        over: an.over.len(),
        places,
        kept: {
            let keep = crate::song::keep_mask(song, &an.events);
            (0..8).filter(|v| keep & 1 << v != 0).collect()
        },
        kept_rows: (song.rows.iter())
            .filter(|r| r.keep)
            .map(|r| r.sound.clone())
            .collect(),
    };

    // Each row that plays, once, in the order it first plays.
    let mut played: Vec<u32> = Vec::new();
    for e in &an.events {
        if !played.contains(&e.row) {
            played.push(e.row);
        }
    }
    let row = |i: u32| &song.rows[i as usize];
    let above_c6 = played
        .iter()
        .map(|&i| row(i))
        .filter(|r| !is_drum(&r.sound) && r.note.unwrap_or(0) > TOP_NOTE)
        .map(|r| format!("{} {}", r.sound, note_name(r.note.unwrap_or(0))))
        .collect();

    // Sweep the pitched notes in start order; a clash is counted once per place in its pattern.
    let mut clashes: Vec<Clash> = Vec::new();
    let mut sounding: Vec<&crate::song::Event> = Vec::new();
    for e in an.events.iter().filter(|e| {
        let r = row(e.row);
        r.note.is_some() && !is_drum(&r.sound) && e.end > e.start
    }) {
        sounding.retain(|o| o.end > e.start);
        let slot = slot_at(e.start);
        for o in &sounding {
            let (a, b) = (row(o.row).note.unwrap_or(0), row(e.row).note.unwrap_or(0));
            let apart = a.abs_diff(b) % 12;
            let beats = (o.end.min(e.end) - e.start) as f64 * tempo_of(slot)
                / (100.0 * 60.0 * TICKS_PER_SECOND);
            if !HARSH.contains(&apart) || beats < CLASH_BEATS {
                continue;
            }
            let kind = if apart == 6 {
                "a tritone"
            } else {
                "a semitone"
            };
            let text = format!(
                "{}: {} against {}, {kind} apart for {} beats",
                place(slot, e.start),
                note_name(b),
                note_name(a),
                short(beats)
            );
            match clashes.iter_mut().find(|c| c.text == text) {
                Some(c) => c.plays += 1,
                None => clashes.push(Clash { text, plays: 1 }),
            }
        }
        sounding.push(e);
    }

    let notes = song
        .arrangement
        .iter()
        .map(|&p| song.patterns[p as usize].notes.len())
        .sum();
    Ok(Report {
        seconds: an.length as f64 / TICKS_PER_SECOND,
        bpm: f64::from(song.tempo) / 100.0,
        slots: song.arrangement.len(),
        notes,
        rows: song.rows.len(),
        voices,
        above_c6,
        clashes,
        loudness: loudness(song, &an, &slots, &played, samples)?,
    })
}

struct Span {
    from: i64,
    to: i64,
}

/// Each arrangement slot's first tick and the next slot's (the song's length for the last).
fn slot_spans(song: &Song, an: &Analysis) -> Vec<Span> {
    let from: Vec<i64> = an
        .slot_steps
        .iter()
        .map(|&s| {
            an.step_ticks
                .get(s as usize)
                .map_or(an.length, |&t| i64::from(t))
        })
        .collect();
    (0..song.arrangement.len())
        .map(|i| Span {
            from: from[i],
            to: from.get(i + 1).copied().unwrap_or(an.length),
        })
        .collect()
}

fn loudness(
    song: &Song,
    an: &Analysis,
    slots: &[Span],
    played: &[u32],
    samples: &[(&str, &[u8])],
) -> Result<Loudness, SongError> {
    // Tick k plays at (k + 1) / 250 s.
    let at = |tick: i64| (tick.max(0) as usize + 1) * SAMPLES_PER_TICK;
    let mix = render(song, samples, an.length)?;
    let slot_levels = slots
        .iter()
        .zip(&song.arrangement)
        .map(|(s, &p)| {
            let (rms, peak) = measure(&mix, at(s.from), at(s.to));
            SlotLevel {
                pattern: song.patterns[p as usize].name.clone(),
                at: s.from as f64 / TICKS_PER_SECOND,
                rms,
                peak,
            }
        })
        .collect();
    let mut sounds: Vec<&str> = Vec::new();
    for &r in played {
        let s = song.rows[r as usize].sound.as_str();
        if !sounds.contains(&s) {
            sounds.push(s);
        }
    }
    let mut by_sound = Vec::new();
    if sounds.len() > 1 {
        for sound in sounds {
            // The same song with every other sound's rows silenced.
            let mut solo = song.clone();
            for r in solo.rows.iter_mut().filter(|r| r.sound != sound) {
                r.vol = 0;
            }
            let pcm = render(&solo, samples, an.length)?;
            let (rms, peak) = measure(&pcm, 0, pcm.len() / 2);
            by_sound.push(SoundLevel {
                sound: sound.into(),
                rms,
                peak,
            });
        }
    }
    let clipped = mix
        .iter()
        .filter(|&&s| s == i16::MAX || s == i16::MIN)
        .count() as u32;
    Ok(Loudness {
        slots: slot_levels,
        sounds: by_sound,
        clipped,
    })
}

/// dBFS (rms, peak) over stereo frames `[from, to)` of interleaved `pcm`.
fn measure(pcm: &[i16], from: usize, to: usize) -> (Option<f64>, Option<f64>) {
    let (a, b) = ((from * 2).min(pcm.len()), (to * 2).min(pcm.len()));
    let (mut squares, mut peak) = (0f64, 0f64);
    for &s in &pcm[a..b] {
        let s = f64::from(s);
        squares += s * s;
        peak = peak.max(s.abs());
    }
    let rms = if b > a {
        (squares / (b - a) as f64).sqrt()
    } else {
        0.0
    };
    let db = |x: f64| (x > 0.0).then(|| 20.0 * (x / 32768.0).log10());
    (db(rms), db(peak))
}

/// Plays `song` once through, alone, and returns its stereo samples up to tick `ticks`.
fn render(song: &Song, samples: &[(&str, &[u8])], ticks: i64) -> Result<Vec<i16>, SongError> {
    let fail = |e: String| SongError(format!("the song did not play: {e}"));
    let mut engine = crate::LuaEngine::new();
    for (name, bytes) in samples {
        engine
            .add_source(name, bytes)
            .map_err(|e| fail(format!("{name}: {e:?}")))?;
    }
    let payload = crate::SourcePayload::Song(crate::song::encode(song)).encode();
    engine
        .add_source("song_check", &payload)
        .map_err(|e| fail(format!("{e:?}")))?;
    engine
        .set_sources(&[(
            "main.luau",
            "score{ song = \"song_check\", loop = false }\nfunction frame() end",
        )])
        .map_err(|e| fail(format!("{e:?}")))?;
    let ticks = ticks.max(0) as usize;
    let frames = (((ticks + 1) as f64 / TICKS_PER_SECOND) * 60.0).ceil() as u32;
    let want = (ticks + 1) * SAMPLES_PER_TICK * 2;
    let mut out = Vec::with_capacity(want);
    for f in 0..frames {
        if out.len() >= want {
            break;
        }
        engine
            .frame(f64::from(f) / 60.0, f)
            .map_err(|e| fail(format!("{e:?}")))?;
        let audio = engine.audio();
        out.extend_from_slice(&audio[..audio.len().min(want - out.len())]);
    }
    out.resize(want, 0);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::song::{Note, Pattern, Row};

    fn note(at: u32, row: u32, len: u32) -> Note {
        Note {
            at,
            row,
            len,
            vel: 127,
            ..Default::default()
        }
    }
    fn row(sound: &str, note: Option<u32>) -> Row {
        Row {
            sound: sound.into(),
            note,
            vol: 100,
            pan: 0,
            keep: false,
        }
    }
    /// One 4/4 bar at 120 BPM over lead C5, lead C#5, bass C2 and kick rows, then an empty bar.
    fn song(notes: Vec<Note>) -> Song {
        Song {
            tempo: 12000,
            voice_mask: 0xff,
            rows: vec![
                row("lead", Some(72)),
                row("lead", Some(73)),
                row("bass", Some(36)),
                row("kick", None),
            ],
            patterns: vec![
                Pattern {
                    name: "verse".into(),
                    length: 192,
                    notes,
                    ..Default::default()
                },
                Pattern {
                    name: "rest".into(),
                    length: 192,
                    ..Default::default()
                },
            ],
            arrangement: vec![0, 1],
            ..Default::default()
        }
    }
    /// Findings only: the lines between the heading and the loudness.
    fn findings(s: &Song) -> Vec<String> {
        let lines = check(s, &[]).unwrap().lines("t");
        let end = lines
            .iter()
            .position(|l| l.starts_with("loudness"))
            .unwrap();
        lines[1..end].to_vec()
    }

    #[test]
    fn names_a_held_semitone_by_pattern_and_beat_and_passes_a_short_one_and_a_drum() {
        let held = song(vec![note(0, 0, 96), note(48, 1, 96), note(0, 3, 96)]);
        assert_eq!(
            findings(&held),
            [
                "voices: at most 3 of 8 sound at once",
                "clashes (advisory: a held clash can be the point): 1",
                "  verse beat 2: C#5 against C5, a semitone apart for 1 beats",
            ]
        );
        let passing = song(vec![note(0, 0, 96), note(72, 1, 24)]);
        assert_eq!(findings(&passing)[1], "clashes: none held");
    }

    #[test]
    fn says_where_the_song_wants_more_voices_than_its_mask_has() {
        let mut crowded = song(vec![note(0, 0, 96), note(48, 2, 96)]);
        crowded.voice_mask = 1;
        let r = check(&crowded, &[]).unwrap();
        assert!(r.failed());
        let line = &r.lines("t")[1];
        assert!(
            line.starts_with("voices: over the 1-voice budget in "),
            "{line}"
        );
        assert!(
            line.contains("so a note there is cut short: verse beat 2"),
            "{line}"
        );
    }

    #[test]
    fn lists_rows_above_c6_and_takes_no_drums_tuning_for_a_pitch() {
        let mut s = song(vec![note(0, 0, 96), note(0, 1, 96), note(0, 2, 96)]);
        s.rows = vec![
            row("lead", Some(86)),
            row("kick", Some(61)),
            row("bass", Some(36)),
        ];
        assert!(check(&s, &[]).unwrap().failed());
        assert_eq!(
            findings(&s)[1..],
            [
                "pitch: 1 rows are above C6, the chip's top pitch, and play as C6: lead D6",
                "clashes: none held",
            ]
        );
    }

    #[test]
    fn names_the_voices_kept_parts_play() {
        let mut s = song(vec![note(0, 3, 96), note(0, 2, 96), note(48, 2, 96)]);
        s.rows[2].keep = true;
        assert_eq!(
            findings(&s)[1],
            "kept: voices 0, 1 play bass, and no automatic sound effect takes them"
        );
    }

    #[test]
    fn the_drums_are_bank_sounds() {
        assert!(crate::bank::DRUMS
            .iter()
            .all(|d| crate::bank::NAMES.contains(d)));
    }

    #[test]
    fn measures_each_slot_and_each_sound_of_the_rendered_song() {
        let s = song(vec![note(0, 0, 192), note(0, 2, 192)]);
        let r = check(&s, &[]).unwrap();
        assert!(!r.failed());
        let lines = r.lines("t");
        let at = lines
            .iter()
            .position(|l| l.starts_with("loudness"))
            .unwrap();
        assert!(
            lines[at + 1].starts_with("  slot  0 verse    0.0 s"),
            "{}",
            lines[at + 1]
        );
        assert!(
            lines[at + 2].starts_with("  slot  1 rest     2.0 s"),
            "{}",
            lines[at + 2]
        );
        let (verse, rest) = (r.loudness.slots[0].rms.unwrap(), r.loudness.slots[1].rms);
        assert!(verse > -40.0, "the bar with notes sounds: {verse}");
        assert!(
            rest.map_or(true, |rest| rest < verse - 20.0),
            "the empty bar holds only tails"
        );
        let sounds: Vec<_> = r.loudness.sounds.iter().map(|s| s.sound.as_str()).collect();
        assert_eq!(sounds, ["lead", "bass"]);
        assert!(r.loudness.sounds[0].rms.unwrap() > -60.0);
        assert_eq!(lines[at + 3], "  by sound, over the whole song:");
        assert!(lines.last().unwrap().starts_with("  clipped: "));
    }
}
