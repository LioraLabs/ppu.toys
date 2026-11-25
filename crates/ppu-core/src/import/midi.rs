//! Standard MIDI File (SMF 0/1) -> a song (`crate::song::Song`). MIDI tick
//! positions round to the nearest 1/48-beat unit; every real tempo change
//! starts a new pattern, sliced further into 8-beat (384-unit) chunks.
//! The CLI packs a `.mid` song source through `song_from_midi`; the Studio's
//! add-source dialog calls `import` through wasm (`songFromMidi`).

use std::collections::{BTreeMap, HashMap};

use midly::{MetaMessage, MidiMessage, Smf, Timing, TrackEventKind};

use crate::song::{Note, Pattern, Row, Song};

/// One note: start and end in MIDI ticks, channel, the channel's first
/// program change in its track, key and velocity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MidiNote {
    pub tick: u64,
    pub end: u64,
    pub ch: u8,
    pub prog: u8,
    pub key: u8,
    pub vel: u8,
}

/// A tempo-change event: microseconds per quarter note from `tick` on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tempo {
    pub tick: u64,
    pub us: u32,
}

/// A parsed .mid, before quantizing.
#[derive(Clone, Debug, PartialEq)]
pub struct Parsed {
    /// Ticks per quarter note; None when the file uses SMPTE division.
    pub ppq: Option<u32>,
    /// Seconds per tick, fixed wall clock; None unless the file uses SMPTE
    /// division (which carries no tempo map).
    pub smpte_sec_per_tick: Option<f64>,
    /// Every tempo-change event, sorted by tick.
    pub tempos: Vec<Tempo>,
    pub notes: Vec<MidiNote>,
}

/// A .mid as a song, and how many placed notes moved onto the 1/48-beat grid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Imported {
    pub song: Song,
    pub rounded: u32,
}

/// .mid bytes -> a song. The plain entry point for the CLI.
pub fn song_from_midi(bytes: &[u8]) -> Result<Song, String> {
    import(bytes).map(|i| i.song)
}

/// .mid bytes -> a song plus the rounded-note count the Studio reports.
pub fn import(bytes: &[u8]) -> Result<Imported, String> {
    let (song, rounded) = to_song(&parse(bytes)?);
    Ok(Imported { song, rounded })
}

/// Reads note on/off pairs, first program changes and the tempo map.
pub fn parse(bytes: &[u8]) -> Result<Parsed, String> {
    let smf = Smf::parse(bytes).map_err(|e| format!("not a MIDI file: {e}"))?;
    let (ppq, smpte_sec_per_tick) = match smf.header.timing {
        Timing::Metrical(t) => (Some(t.as_int() as u32), None),
        Timing::Timecode(fps, sub) => (None, Some(1.0 / (fps.as_int() as f64 * sub as f64))),
    };
    if ppq == Some(0) || smpte_sec_per_tick.is_some_and(|s| !s.is_finite()) {
        return Err("MIDI file has no time division".into());
    }

    let mut tempos = Vec::new();
    let mut notes = Vec::new();
    for track in &smf.tracks {
        let mut prog: HashMap<u8, u8> = HashMap::new(); // channel -> first program change
        let mut raw: Vec<(u64, u64, u8, u8, u8)> = Vec::new(); // tick, end, ch, key, vel
                                                               // (ch, key) -> stack of open notes, in first-seen order.
        let mut pending: Vec<((u8, u8), Vec<(u64, u8)>)> = Vec::new();
        let mut tick = 0u64;
        for ev in track {
            tick += ev.delta.as_int() as u64;
            match ev.kind {
                TrackEventKind::Meta(MetaMessage::Tempo(us)) => tempos.push(Tempo {
                    tick,
                    us: us.as_int(),
                }),
                TrackEventKind::Midi { channel, message } => {
                    let ch = channel.as_int();
                    let (key, vel, on) = match message {
                        MidiMessage::NoteOn { key, vel } => (key.as_int(), vel.as_int(), vel > 0),
                        MidiMessage::NoteOff { key, .. } => (key.as_int(), 0, false),
                        MidiMessage::ProgramChange { program } => {
                            prog.entry(ch).or_insert(program.as_int());
                            continue;
                        }
                        _ => continue,
                    };
                    let slot = match pending.iter().position(|(k, _)| *k == (ch, key)) {
                        Some(i) => i,
                        None if on => {
                            pending.push(((ch, key), Vec::new()));
                            pending.len() - 1
                        }
                        None => continue,
                    };
                    if on {
                        pending[slot].1.push((tick, vel));
                    } else if let Some((start, v)) = pending[slot].1.pop() {
                        raw.push((start, tick, ch, key, v));
                    }
                }
                _ => {}
            }
        }
        // Notes still held at the track's end close there.
        for ((ch, key), stack) in pending {
            for (start, v) in stack {
                raw.push((start, tick, ch, key, v));
            }
        }
        notes.extend(raw.into_iter().map(|(tick, end, ch, key, vel)| MidiNote {
            tick,
            end,
            ch,
            prog: prog.get(&ch).copied().unwrap_or(0),
            key,
            vel,
        }));
    }
    // Ticks are beat-relative whatever the tempo map says; the map is kept whole.
    tempos.sort_by_key(|t| t.tick);
    if notes.is_empty() {
        return Err("MIDI file has no notes".into());
    }
    Ok(Parsed {
        ppq,
        smpte_sec_per_tick,
        tempos,
        notes,
    })
}

// General MIDI -> built-in sounds (kit.lua's BANK): one sound per program
// family of 8, and drum-channel keys to drum presets (unlisted keys -> hat).
const GM_FAMILY: [&str; 16] = [
    "piano", "bell", "organ", "pluck", "bass", "strings", "strings", "lead", "flute", "flute",
    "lead", "strings", "bell", "pluck", "pluck", "lead",
];

fn gm_drum(key: u8) -> &'static str {
    match key {
        35 | 36 => "kick",
        38 | 40 => "snare",
        39 => "clap",
        41 | 43 | 45 | 47 | 48 | 50 => "tom",
        46 | 51 | 53 | 59 => "ohat",
        49 | 52 | 55 | 57 => "crash",
        _ => "hat", // 37, 42, 44, 54, 56 and every unlisted key
    }
}

/// A, B, ..., Z, AA, AB, ... (letters, so pattern names stay stable and readable).
fn pattern_name(i: usize) -> String {
    let head = if i >= 26 {
        pattern_name(i / 26 - 1)
    } else {
        String::new()
    };
    head + &char::from(b'A' + (i % 26) as u8).to_string()
}

const UNITS_PER_BEAT: u64 = 48;
const PATTERN_UNITS: u64 = UNITS_PER_BEAT * 8;
const DEFAULT_CENTI_BPM: u32 = 12000; // 120 BPM before any tempo event

/// A MIDI tick -> its rounded 1/48-beat unit, and whether it landed exactly on the grid.
fn to_unit(m: &Parsed, tick: u64) -> (u64, bool) {
    match m.ppq {
        Some(ppq) => {
            let (scaled, ppq) = (tick * UNITS_PER_BEAT, ppq as u64);
            ((2 * scaled + ppq) / (2 * ppq), scaled % ppq == 0) // round half up
        }
        None => {
            // 96 units/s at wall-clock 120 BPM
            let raw =
                tick as f64 * m.smpte_sec_per_tick.unwrap_or(0.0) * (UNITS_PER_BEAT * 2) as f64;
            ((raw + 0.5).floor() as u64, raw.fract() == 0.0)
        }
    }
}

/// us/quarter-note -> centi-BPM, clamped to the engine's tempo range.
fn centi_bpm(us: u32) -> u32 {
    (6e9 / us as f64 + 0.5).floor().clamp(100.0, 40000.0) as u32
}

/// Quantize a parsed .mid into a song: ticks round to the nearest 1/48-beat
/// unit, every real tempo change starts a new pattern (sliced further into
/// 8-beat chunks), and every distinct (sound, key) row is kept. The count is
/// the notes placed (after merging duplicate hits at the same row and start)
/// whose rounded start or end moved off the file's exact tick.
pub fn to_song(m: &Parsed) -> (Song, u32) {
    struct RowInfo {
        sound: &'static str,
        key: u8,
        drum: bool,
        first: usize,
    }
    struct Event {
        row_id: (&'static str, Option<u8>),
        start: u64,
        end: u64,
        vel: u8,
        exact: bool,
    }
    let mut row_info: Vec<((&'static str, Option<u8>), RowInfo)> = Vec::new();
    let mut events = Vec::new();
    let mut sorted: Vec<&MidiNote> = m.notes.iter().collect();
    sorted.sort_by_key(|n| n.tick);
    for n in sorted {
        let drum = n.ch == 9;
        let sound = if drum {
            gm_drum(n.key)
        } else {
            GM_FAMILY[(n.prog >> 3) as usize]
        };
        let id = (sound, (!drum).then_some(n.key));
        if !row_info.iter().any(|(k, _)| *k == id) {
            let first = row_info.len();
            row_info.push((
                id,
                RowInfo {
                    sound,
                    key: n.key,
                    drum,
                    first,
                },
            ));
        }
        let (s, se) = to_unit(m, n.tick);
        let (e, ee) = to_unit(m, n.end);
        events.push(Event {
            row_id: id,
            start: s,
            end: e,
            vel: n.vel,
            exact: se && ee,
        });
    }

    // Rows: first appearance of the sound, high notes above low within it.
    let sound_order = |sound: &str| {
        row_info
            .iter()
            .find(|(_, r)| r.sound == sound)
            .map_or(0, |(_, r)| r.first)
    };
    let mut sorted_rows: Vec<&((&'static str, Option<u8>), RowInfo)> = row_info.iter().collect();
    sorted_rows.sort_by(|a, z| {
        sound_order(a.1.sound)
            .cmp(&sound_order(z.1.sound))
            .then(z.1.key.cmp(&a.1.key))
    });
    let row_index =
        |id: (&'static str, Option<u8>)| sorted_rows.iter().position(|r| r.0 == id).unwrap_or(0);
    let rows: Vec<Row> = sorted_rows
        .iter()
        .map(|(_, r)| Row {
            sound: r.sound.to_string(),
            note: (!r.drum).then_some(r.key as u32),
            vol: 127,
            pan: 0,
            keep: false,
        })
        .collect();

    // The song ends past every note's start + its effective length (>= 1
    // unit, so a zero-tick hit still has room), rounded up to a whole beat:
    // a note at exactly the pattern's length is refused by the engine.
    let max_span = events
        .iter()
        .map(|e| e.start + (e.end.saturating_sub(e.start)).max(1))
        .max()
        .unwrap_or(0);
    let song_end = UNITS_PER_BEAT.max(max_span.div_ceil(UNITS_PER_BEAT) * UNITS_PER_BEAT);

    // Tempo regions: every real change (several at one unit -> the last
    // wins; a change equal to the running tempo, or at/after the song end,
    // is not one) starts a new region.
    let mut change_at: BTreeMap<u64, u32> = BTreeMap::new();
    if m.ppq.is_some() {
        for t in &m.tempos {
            let (u, _) = to_unit(m, t.tick);
            if u < song_end {
                change_at.insert(u, centi_bpm(t.us));
            }
        }
    }
    let mut regions: Vec<(u64, u64, u32)> = Vec::new(); // start, end, centi-BPM
    let (mut running, mut region_start) = (DEFAULT_CENTI_BPM, 0u64);
    for (&u, &centi) in &change_at {
        if u == 0 {
            running = centi; // overrides the default; region 0 still starts at 0
            continue;
        }
        if centi == running {
            continue;
        }
        regions.push((region_start, u, running));
        region_start = u;
        running = centi;
    }
    regions.push((region_start, song_end, running));
    let song_tempo = regions[0].2;

    // Each region splits into 8-beat chunks, the region's last one shorter
    // if it doesn't divide evenly.
    let mut chunks: Vec<(u64, u64, Option<u32>)> = Vec::new(); // start, length, tempo
    for &(start, end, centi) in &regions {
        let mut cs = start;
        while cs < end {
            chunks.push((
                cs,
                PATTERN_UNITS.min(end - cs),
                (centi != song_tempo).then_some(centi),
            ));
            cs += PATTERN_UNITS;
        }
    }
    let chunk_at = |u: u64| chunks.iter().rposition(|c| c.0 <= u).unwrap_or(0);

    // Notes per chunk; a duplicate (row, at) keeps the loudest, then the longest.
    struct Placed {
        len: u64,
        vel: u8,
        exact: bool,
    }
    let mut per_chunk: Vec<BTreeMap<(u64, usize), Placed>> =
        (0..chunks.len()).map(|_| BTreeMap::new()).collect();
    for e in &events {
        let row = row_index(e.row_id);
        let ci = chunk_at(e.start);
        let at = e.start - chunks[ci].0;
        let len = e.end.saturating_sub(e.start).max(1);
        let bucket = &mut per_chunk[ci];
        let keep = match bucket.get(&(at, row)) {
            None => true,
            Some(had) => e.vel > had.vel || (e.vel == had.vel && len > had.len),
        };
        if keep {
            bucket.insert(
                (at, row),
                Placed {
                    len,
                    vel: e.vel,
                    exact: e.exact,
                },
            );
        }
    }
    let rounded = per_chunk
        .iter()
        .flat_map(|b| b.values())
        .filter(|n| !n.exact)
        .count() as u32;

    // Identical chunks (same length, tempo, notes) share one pattern.
    let mut patterns: Vec<Pattern> = Vec::new();
    let mut arrangement = Vec::new();
    for (ci, &(_, length, tempo)) in chunks.iter().enumerate() {
        let notes: Vec<Note> = per_chunk[ci]
            .iter()
            .map(|(&(at, row), n)| Note {
                at: at as u32,
                row: row as u32,
                len: n.len as u32,
                vel: n.vel as u32,
                ..Note::default()
            })
            .collect();
        let found = patterns
            .iter()
            .position(|p| p.length == length as u32 && p.tempo == tempo && p.notes == notes);
        let idx = found.unwrap_or_else(|| {
            patterns.push(Pattern {
                name: pattern_name(patterns.len()),
                length: length as u32,
                tempo,
                notes,
            });
            patterns.len() - 1
        });
        arrangement.push(idx as u32);
    }

    let song = Song {
        tempo: song_tempo,
        swing: 0,
        key: 0,
        voice_mask: 0xff,
        rows,
        patterns,
        arrangement,
        loop_start: 0,
    };
    (song, rounded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::song::{compile, decode, encode};
    use crate::song_analyze::analyze;

    fn be32(v: u32) -> [u8; 4] {
        v.to_be_bytes()
    }
    fn smf(division: u16, tracks: &[Vec<u8>]) -> Vec<u8> {
        let mut b = b"MThd".to_vec();
        b.extend(be32(6));
        b.extend([0, 1]);
        b.extend((tracks.len() as u16).to_be_bytes());
        b.extend(division.to_be_bytes());
        for t in tracks {
            b.extend(b"MTrk");
            b.extend(be32(t.len() as u32));
            b.extend(t);
        }
        b
    }
    /// Variable-length quantity, MIDI's delta-time encoding.
    fn vlq(mut v: u64) -> Vec<u8> {
        let mut bytes = vec![(v & 0x7f) as u8];
        v >>= 7;
        while v > 0 {
            bytes.insert(0, (v & 0x7f) as u8 | 0x80);
            v >>= 7;
        }
        bytes
    }
    type Ev = (u64, Vec<u8>);
    /// One track's bytes from absolute-tick events (stable-sorted).
    fn track(mut events: Vec<Ev>) -> Vec<u8> {
        events.sort_by_key(|e| e.0);
        let (mut out, mut prev) = (Vec::new(), 0);
        for (tick, bytes) in events {
            out.extend(vlq(tick - prev));
            out.extend(bytes);
            prev = tick;
        }
        out.extend([0, 0xff, 0x2f, 0]);
        out
    }
    fn tempo_ev(tick: u64, us: u32) -> Ev {
        (
            tick,
            vec![0xff, 0x51, 3, (us >> 16) as u8, (us >> 8) as u8, us as u8],
        )
    }
    fn on(tick: u64, ch: u8, key: u8, vel: u8) -> Ev {
        (tick, vec![0x90 | ch, key, vel])
    }
    fn off(tick: u64, ch: u8, key: u8) -> Ev {
        (tick, vec![0x80 | ch, key, 0])
    }
    fn n(tick: u64, end: u64, ch: u8, prog: u8, key: u8, vel: u8) -> MidiNote {
        MidiNote {
            tick,
            end,
            ch,
            prog,
            key,
            vel,
        }
    }

    // ppq 48 makes a MIDI tick equal one unit, so fixtures are written in units.
    fn midi(notes: Vec<MidiNote>, tempos: Vec<Tempo>) -> Parsed {
        Parsed {
            ppq: Some(48),
            smpte_sec_per_tick: None,
            tempos,
            notes,
        }
    }
    fn note(tick: u64, len: u64, key: u8) -> MidiNote {
        n(tick, tick + len, 0, 0, key, 100)
    }
    fn t(tick: u64, us: u32) -> Tempo {
        Tempo { tick, us }
    }
    fn row(sound: &str, note: Option<u32>) -> Row {
        Row {
            sound: sound.into(),
            note,
            vol: 127,
            pan: 0,
            keep: false,
        }
    }
    fn placed(at: u32, row: u32, len: u32, vel: u32) -> Note {
        Note {
            at,
            row,
            len,
            vel,
            ..Note::default()
        }
    }

    #[test]
    fn pairs_note_on_off_with_running_status_and_keeps_the_tempo_map() {
        #[rustfmt::skip]
        let tr = vec![
            0, 0xff, 0x51, 3, 0x09, 0x27, 0xc0, // 600000 us/quarter = 100 BPM
            0, 0xc0, 33, // program 33 (fingered bass)
            0, 0x90, 60, 100, // C4 on at tick 0
            0x83, 0x60, 60, 0, // running status, vel 0 = off at tick 480
            0, 64, 64, // E4 on at 480 (running status)
            0, 0xff, 0x51, 3, 0x07, 0xa1, 0x20, // a second tempo change, also kept
            0x81, 0x70, 0x80, 64, 0, // explicit off 240 ticks later
            0, 0xff, 0x2f, 0,
        ];
        let m = parse(&smf(480, &[tr])).unwrap();
        assert_eq!(m.ppq, Some(480));
        assert_eq!(m.smpte_sec_per_tick, None);
        assert_eq!(m.tempos, vec![t(0, 600000), t(480, 500000)]);
        assert_eq!(
            m.notes,
            vec![n(0, 480, 0, 33, 60, 100), n(480, 720, 0, 33, 64, 64)]
        );
    }

    #[test]
    fn no_tempo_events_without_a_tempo_meta_message() {
        let tr = vec![0, 0x99, 36, 90, 0x60, 0x89, 36, 0, 0, 0xff, 0x2f, 0];
        let m = parse(&smf(96, &[tr])).unwrap();
        assert_eq!(m.ppq, Some(96));
        assert!(m.tempos.is_empty());
        assert_eq!(
            (
                m.notes[0].ch,
                m.notes[0].key,
                m.notes[0].tick,
                m.notes[0].end
            ),
            (9, 36, 0, 96)
        );
    }

    #[test]
    fn refuses_non_midi_bytes_and_note_less_files() {
        assert!(parse(&[1, 2, 3]).unwrap_err().contains("not a MIDI file"));
        assert!(parse(&smf(96, &[vec![0, 0xff, 0x2f, 0]]))
            .unwrap_err()
            .contains("no notes"));
    }

    #[test]
    fn reads_smpte_division_as_a_fixed_wall_clock_rate() {
        // -25 fps (0x100 - 25 = 0xE7), 40 subframes: 1000 ticks/s.
        let division = (0xe7 << 8) | 40;
        let tr = track(vec![on(0, 0, 60, 100), off(1000, 0, 60)]);
        let m = parse(&smf(division, &[tr])).unwrap();
        assert_eq!(m.ppq, None);
        assert!((m.smpte_sec_per_tick.unwrap() - 0.001).abs() < 1e-9);
        assert!(m.tempos.is_empty());
    }

    #[test]
    fn maps_gm_programs_and_drum_keys_to_rows() {
        let (song, rounded) = to_song(&midi(
            vec![
                note(0, 12, 60),           // piano C4
                n(0, 12, 9, 0, 36, 100),   // kick
                n(24, 36, 9, 0, 35, 40),   // also kick: same row
                n(24, 36, 9, 0, 38, 20),   // snare
                n(48, 60, 0, 33, 45, 100), // bass family
            ],
            vec![],
        ));
        assert_eq!(rounded, 0);
        assert_eq!(
            song.rows,
            vec![
                row("piano", Some(60)),
                row("kick", None),
                row("snare", None),
                row("bass", Some(45))
            ]
        );
        assert_eq!((song.voice_mask, song.swing, song.key), (0xff, 0, 0));
    }

    #[test]
    fn keeps_a_duplicate_hit_only_as_the_loudest_then_the_longest() {
        let (song, _) = to_song(&midi(
            vec![
                n(0, 6, 0, 0, 60, 30),
                n(0, 12, 0, 0, 60, 110),
                n(0, 4, 0, 0, 60, 110),
            ],
            vec![],
        ));
        assert_eq!(song.patterns[0].notes, vec![placed(0, 0, 12, 110)]);
    }

    #[test]
    fn splits_into_384_unit_patterns_and_dedupes_identical_ones() {
        let bars = |at| [note(at, 12, 60), note(at + 200, 12, 62)];
        let mut notes = bars(0).to_vec();
        notes.extend(bars(384));
        notes.push(note(900, 12, 64));
        let (song, _) = to_song(&midi(notes, vec![]));
        let lengths: Vec<u32> = song.patterns.iter().map(|p| p.length).collect();
        assert_eq!(lengths, vec![384, 144]); // 912 rounds up to a beat, no further
        assert_eq!(song.arrangement, vec![0, 0, 1]);
    }

    #[test]
    fn starts_a_new_pattern_only_at_a_real_tempo_change() {
        let (song, _) = to_song(&midi(
            vec![note(0, 12, 60), note(400, 12, 60)],
            vec![
                t(0, 500000),     // 120 BPM: the default, not a change
                t(200, 500000),   // still 120: not a change
                t(384, 400000),   // 150 BPM: a real change
                t(10000, 300000), // at/after the song's end: ignored
            ],
        ));
        assert_eq!(song.tempo, 12000);
        let tempos: Vec<Option<u32>> = song.patterns.iter().map(|p| p.tempo).collect();
        assert_eq!(tempos, vec![None, Some(15000)]);
    }

    #[test]
    fn keeps_only_the_last_of_several_tempo_changes_on_one_unit() {
        let raw = Parsed {
            ppq: Some(480),
            smpte_sec_per_tick: None,
            tempos: vec![t(3840, 400000), t(3844, 800000)], // both round to unit 384: 75 BPM wins
            notes: vec![note(0, 160, 60), note(3840, 160, 60)],
        };
        let (song, _) = to_song(&raw);
        assert_eq!(song.patterns[1].tempo, Some(7500));
    }

    #[test]
    fn rounds_an_off_grid_tick_to_the_nearest_unit_and_counts_it() {
        let m = parse(&smf(480, &[track(vec![on(7, 0, 60, 100), off(20, 0, 60)])])).unwrap();
        let (song, rounded) = to_song(&m);
        assert_eq!(rounded, 1); // 7 * 48 % 480 != 0
        assert_eq!(song.patterns[0].notes[0].at, 1); // round(0.7)
    }

    fn encodes(song: &Song) -> Vec<u8> {
        compile(song).expect("the engine accepts the song");
        encode(song)
    }

    /// An 8th-note triplet, a 16th sextuplet, and a tempo change from 120 to
    /// 90 BPM at beat 8 (exactly one 384-unit pattern); the song ends at beat 10.
    #[test]
    fn lands_every_note_exactly_splits_at_the_tempo_change_and_round_trips() {
        let mut ev = vec![
            tempo_ev(0, 500000),
            on(0, 0, 60, 1),
            off(160, 0, 60),
            on(160, 0, 60, 37),
            off(320, 0, 60),
            on(320, 0, 60, 100),
            off(480, 0, 60),
        ];
        for i in 0..6 {
            ev.push(on(480 + i * 80, 9, 42, 64));
            ev.push(off(480 + (i + 1) * 80, 9, 42));
        }
        ev.extend([
            tempo_ev(3840, 666667),
            on(3840, 0, 60, 64),
            off(4000, 0, 60),
            on(4000, 0, 60, 64),
            off(4160, 0, 60),
            on(4160, 0, 60, 64),
            off(4320, 0, 60),
            on(4320, 0, 60, 127),
            off(4480, 0, 60),
        ]);
        let Imported { song, rounded } = import(&smf(480, &[track(ev)])).unwrap();
        assert_eq!(rounded, 0);
        assert_eq!(song.tempo, 12000);
        assert_eq!(song.patterns.len(), 2);
        assert_eq!(
            (song.patterns[0].length, song.patterns[0].tempo),
            (384, None)
        );
        assert_eq!(
            (song.patterns[1].length, song.patterns[1].tempo),
            (96, Some(9000))
        );

        let piano = song.rows.iter().position(|r| r.sound == "piano").unwrap() as u32;
        let hat = song.rows.iter().position(|r| r.sound == "hat").unwrap() as u32;
        let ats = |p: usize, r: u32| -> Vec<u32> {
            song.patterns[p]
                .notes
                .iter()
                .filter(|n| n.row == r)
                .map(|n| n.at)
                .collect()
        };
        assert_eq!(ats(0, piano), vec![0, 16, 32]);
        assert_eq!(ats(0, hat), vec![48, 56, 64, 72, 80, 88]);
        let after: Vec<(u32, u32)> = song.patterns[1]
            .notes
            .iter()
            .map(|n| (n.at, n.vel))
            .collect();
        assert_eq!(after, vec![(0, 64), (16, 64), (32, 64), (48, 127)]);

        let bytes = encodes(&song);
        assert_eq!(decode(&bytes).unwrap(), song);

        // Engine ticks (4 ms) from the true wall-clock time: 8 beats at 120
        // BPM take 4 s, then each beat takes 60/90 s.
        let region1 = |u: f64| (u / 96.0 * 250.0).round() as u64;
        let region2 = |u: f64| ((4.0 + (u - 384.0) / 72.0) * 250.0).round() as u64;
        let starts: Vec<u64> = analyze(&song)
            .unwrap()
            .events
            .iter()
            .map(|e| e.start as u64)
            .collect();
        let mut want: Vec<u64> = [0.0, 16.0, 32.0, 48.0, 56.0, 64.0, 72.0, 80.0, 88.0]
            .map(region1)
            .to_vec();
        want.extend([384.0, 400.0, 416.0, 432.0].map(region2));
        assert_eq!(starts, want);
    }

    #[test]
    fn keeps_a_zero_length_hit_on_a_beat_boundary_as_the_last_note() {
        let ev = vec![
            on(0, 0, 60, 100),
            off(240, 0, 60),
            on(960, 9, 36, 100),
            off(960, 9, 36),
        ];
        let Imported { song, rounded } = import(&smf(480, &[track(ev)])).unwrap();
        assert_eq!(rounded, 0);
        for p in &song.patterns {
            assert!(p.notes.iter().all(|n| n.at < p.length));
        }
        encodes(&song);
    }

    #[test]
    fn keeps_a_final_note_rounding_onto_a_beat_boundary() {
        let ev = vec![
            on(0, 0, 60, 100),
            off(240, 0, 60),
            on(479, 0, 62, 100),
            off(480, 0, 62),
        ];
        let Imported { song, rounded } = import(&smf(480, &[track(ev)])).unwrap();
        assert_eq!(rounded, 1); // the 479 start rounds off-grid
        for p in &song.patterns {
            assert!(p.notes.iter().all(|n| n.at < p.length));
        }
        encodes(&song);
    }

    #[test]
    fn song_from_midi_refuses_a_malformed_file() {
        assert!(song_from_midi(b"MThd garbage").is_err());
    }
}
