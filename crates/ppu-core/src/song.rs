//! PSNG: the song format, its timing map, compiler and voice allocator.
//!
//! A song is data a toy carries as a `song` source: rows (instrument slots),
//! patterns (step data), and an arrangement (play order). This is the one
//! place that decodes/encodes the PSNG bytes, maps a song position to an
//! engine tick, turns a song into a flat list of playable events, and
//! assigns those events to the 8 S-DSP voices. Everything else (the web
//! panel, native playback) is a caller of this module.

/// Units per beat for all song positions (`at`, `len`, pattern `length`):
/// 1/48 beat. A 16th note is 12 units, a 32nd is 6, an 8th triplet is 16, a
/// 16th triplet is 8.
pub const UNITS_PER_BEAT: u32 = 48;

/// A 16th note, in [`UNITS_PER_BEAT`] units: the swing grid's step.
const UNITS_PER_16TH: u64 = UNITS_PER_BEAT as u64 / 4;

/// The largest total note count (summed over the arrangement) `Song::validate`
/// allows, so `compile` never allocates an unbounded events `Vec`.
const MAX_EVENTS: u64 = 1_000_000;

/// Valid song/pattern tempo, in centi-BPM (1..400 BPM).
const TEMPO_RANGE: std::ops::RangeInclusive<u32> = 100..=40000;

/// A song: tempo/swing/key, the voices it may use, its rows, patterns and
/// play order.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Song {
    /// Centi-BPM (12000 = 120 BPM). Valid 100..=40000.
    pub tempo: u32,
    /// Percent. Valid 0..=75.
    pub swing: u32,
    /// Key signature; opaque to the engine, round-trips as given.
    pub key: i32,
    /// Bit v set = the song may use voice v.
    pub voice_mask: u8,
    pub rows: Vec<Row>,
    pub patterns: Vec<Pattern>,
    /// Pattern index per arrangement slot, in play order.
    pub arrangement: Vec<u32>,
}

/// One instrument slot a pattern's notes can point at.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Row {
    pub sound: String,
    /// MIDI note, valid 0..=127.
    pub note: Option<u32>,
    /// 0..=127.
    pub vol: u32,
    /// -100..=100, maps to -1..1.
    pub pan: i32,
}

/// A block of step data: a name, a length in units, an optional tempo
/// override, and its notes.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Pattern {
    pub name: String,
    /// Units, >= 1.
    pub length: u32,
    /// Centi-BPM; replaces the song's tempo while this pattern plays.
    pub tempo: Option<u32>,
    pub notes: Vec<Note>,
}

/// One note in a pattern.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Note {
    /// Units from the pattern's start; must be < the pattern's length.
    pub at: u32,
    /// Index into the song's rows.
    pub row: u32,
    /// Units, >= 1.
    pub len: u32,
    /// 0..=127.
    pub vel: u32,
    /// Pins the note to a voice, 0..=7.
    pub voice: Option<u32>,
    /// 4 ms engine ticks, added to the computed start.
    pub nudge: i32,
}

/// A decode/encode/validate/compile failure, with a human-readable message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SongError(pub String);

impl std::fmt::Display for SongError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for SongError {}

// --- varint / zigzag -------------------------------------------------------

fn zigzag_encode(n: i32) -> u32 {
    ((n << 1) ^ (n >> 31)) as u32
}

fn zigzag_decode(z: u32) -> i32 {
    ((z >> 1) as i32) ^ -((z & 1) as i32)
}

fn write_varint(out: &mut Vec<u8>, mut v: u32) {
    loop {
        let byte = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

enum VarintErr {
    Truncated,
    Malformed,
}

/// Reads one LEB128 varint starting at `*pos`, advancing it. Rejects an
/// encoding longer than 5 bytes or one that overflows `u32`.
fn read_varint(bytes: &[u8], pos: &mut usize) -> Result<u32, VarintErr> {
    let mut result: u64 = 0;
    let mut shift = 0u32;
    for _ in 0..5 {
        if *pos >= bytes.len() {
            return Err(VarintErr::Truncated);
        }
        let byte = bytes[*pos];
        *pos += 1;
        result |= ((byte & 0x7f) as u64) << shift;
        if byte & 0x80 == 0 {
            return if result > u32::MAX as u64 {
                Err(VarintErr::Malformed)
            } else {
                Ok(result as u32)
            };
        }
        shift += 7;
    }
    Err(VarintErr::Malformed)
}

/// A cursor over one chunk's body, producing messages scoped to that chunk
/// (e.g. "HEAD chunk truncated", "PATT chunk 2 malformed").
struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
    ctx: String,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8], ctx: &str) -> Self {
        Cursor {
            bytes,
            pos: 0,
            ctx: ctx.to_string(),
        }
    }

    fn truncated(&self) -> SongError {
        SongError(format!("{} truncated", self.ctx))
    }

    fn malformed(&self) -> SongError {
        SongError(format!("{} malformed", self.ctx))
    }

    fn u8(&mut self) -> Result<u8, SongError> {
        if self.pos >= self.bytes.len() {
            return Err(self.truncated());
        }
        let b = self.bytes[self.pos];
        self.pos += 1;
        Ok(b)
    }

    fn varint(&mut self) -> Result<u32, SongError> {
        match read_varint(self.bytes, &mut self.pos) {
            Ok(v) => Ok(v),
            Err(VarintErr::Truncated) => Err(self.truncated()),
            Err(VarintErr::Malformed) => Err(self.malformed()),
        }
    }

    fn zigzag(&mut self) -> Result<i32, SongError> {
        Ok(zigzag_decode(self.varint()?))
    }

    fn string(&mut self) -> Result<String, SongError> {
        let len = self.varint()? as usize;
        if len > self.bytes.len().saturating_sub(self.pos) {
            return Err(self.truncated());
        }
        let s = &self.bytes[self.pos..self.pos + len];
        self.pos += len;
        String::from_utf8(s.to_vec()).map_err(|_| self.malformed())
    }

    /// The bytes left unread in this chunk's body.
    fn remaining(&self) -> usize {
        self.bytes.len() - self.pos
    }

    fn finish(&self) -> Result<(), SongError> {
        if self.pos != self.bytes.len() {
            Err(SongError(format!("{} has trailing bytes", self.ctx)))
        } else {
            Ok(())
        }
    }
}

fn parse_head(body: &[u8], ctx: &str) -> Result<(u32, u32, i32, u8), SongError> {
    let mut c = Cursor::new(body, ctx);
    let ticks_per_beat = c.varint()?;
    if ticks_per_beat != UNITS_PER_BEAT {
        return Err(SongError(format!("{ctx}: ticks per beat must be 48")));
    }
    let tempo = c.varint()?;
    let swing = c.varint()?;
    let key = c.zigzag()?;
    let voice_mask = c.u8()?;
    c.finish()?;
    Ok((tempo, swing, key, voice_mask))
}

fn parse_rows(body: &[u8], ctx: &str) -> Result<Vec<Row>, SongError> {
    let mut c = Cursor::new(body, ctx);
    let count = c.varint()?;
    // Never trust a raw wire count to size an allocation: cap it by the
    // bytes actually left (each row costs >= 1 byte), so a huge count on a
    // short body can't abort the process before the truncation check below
    // ever runs.
    let mut rows = Vec::with_capacity((count as usize).min(c.remaining()));
    for _ in 0..count {
        let sound = c.string()?;
        let note_plus1 = c.varint()?;
        let note = if note_plus1 == 0 {
            None
        } else {
            Some(note_plus1 - 1)
        };
        let vol = c.varint()?;
        let pan = c.zigzag()?;
        rows.push(Row {
            sound,
            note,
            vol,
            pan,
        });
    }
    c.finish()?;
    Ok(rows)
}

fn parse_patt(body: &[u8], ctx: &str) -> Result<Pattern, SongError> {
    let mut c = Cursor::new(body, ctx);
    let name = c.string()?;
    let length = c.varint()?;
    let tempo_raw = c.varint()?;
    let tempo = if tempo_raw == 0 {
        None
    } else {
        Some(tempo_raw)
    };
    let note_count = c.varint()?;
    // See parse_rows: cap the raw wire count by the bytes left.
    let mut notes = Vec::with_capacity((note_count as usize).min(c.remaining()));
    let mut prev: i64 = 0;
    for _ in 0..note_count {
        let delta = c.zigzag()? as i64;
        let at = (prev + delta) as u32; // wrapping is fine for u32-range values
        prev = at as i64;
        let packed = c.varint()?;
        let pinned = packed & 1 != 0;
        let nudged = (packed >> 1) & 1 != 0;
        let row = packed >> 2;
        let len = c.varint()?;
        let vel = c.varint()?;
        let voice = if pinned { Some(c.u8()? as u32) } else { None };
        let nudge = if nudged { c.zigzag()? } else { 0 };
        notes.push(Note {
            at,
            row,
            len,
            vel,
            voice,
            nudge,
        });
    }
    c.finish()?;
    Ok(Pattern {
        name,
        length,
        tempo,
        notes,
    })
}

fn parse_arrg(body: &[u8], ctx: &str) -> Result<Vec<u32>, SongError> {
    let mut c = Cursor::new(body, ctx);
    let count = c.varint()?;
    // See parse_rows: cap the raw wire count by the bytes left.
    let mut arr = Vec::with_capacity((count as usize).min(c.remaining()));
    for _ in 0..count {
        arr.push(c.varint()?);
    }
    c.finish()?;
    Ok(arr)
}

/// Decodes PSNG bytes into a [`Song`]. Structural only: no range checks
/// (that's [`Song::validate`]).
pub fn decode(bytes: &[u8]) -> Result<Song, SongError> {
    if bytes.len() < 5 || &bytes[0..4] != b"PSNG" {
        return Err(SongError("not a PSNG song".into()));
    }
    let version = bytes[4];
    if version != 1 {
        return Err(SongError(format!("unsupported PSNG version {version}")));
    }

    let mut pos = 5usize;
    let mut head: Option<(u32, u32, i32, u8)> = None;
    let mut rows: Option<Vec<Row>> = None;
    let mut arrangement: Option<Vec<u32>> = None;
    let mut patterns: Vec<Pattern> = Vec::new();

    while pos < bytes.len() {
        if pos + 4 > bytes.len() {
            return Err(SongError("chunk header truncated".into()));
        }
        let tag: [u8; 4] = bytes[pos..pos + 4].try_into().unwrap();
        pos += 4;
        let body_len = match read_varint(bytes, &mut pos) {
            Ok(v) => v as usize,
            Err(_) => return Err(SongError("chunk header truncated".into())),
        };

        let label = if &tag == b"PATT" {
            format!("PATT chunk {}", patterns.len() + 1)
        } else {
            format!("{} chunk", String::from_utf8_lossy(&tag))
        };

        if body_len > bytes.len().saturating_sub(pos) {
            return Err(SongError(format!("{label} truncated")));
        }
        let body = &bytes[pos..pos + body_len];
        pos += body_len;

        match &tag {
            b"HEAD" => {
                if head.is_some() {
                    return Err(SongError("duplicate HEAD chunk".into()));
                }
                head = Some(parse_head(body, &label)?);
            }
            b"ROWS" => {
                if rows.is_some() {
                    return Err(SongError("duplicate ROWS chunk".into()));
                }
                rows = Some(parse_rows(body, &label)?);
            }
            b"ARRG" => {
                if arrangement.is_some() {
                    return Err(SongError("duplicate ARRG chunk".into()));
                }
                arrangement = Some(parse_arrg(body, &label)?);
            }
            b"PATT" => {
                patterns.push(parse_patt(body, &label)?);
            }
            _ => {} // unknown tag: skipped
        }
    }

    let (tempo, swing, key, voice_mask) =
        head.ok_or_else(|| SongError("missing HEAD chunk".into()))?;
    let rows = rows.ok_or_else(|| SongError("missing ROWS chunk".into()))?;
    let arrangement = arrangement.ok_or_else(|| SongError("missing ARRG chunk".into()))?;

    Ok(Song {
        tempo,
        swing,
        key,
        voice_mask,
        rows,
        patterns,
        arrangement,
    })
}

fn write_chunk(out: &mut Vec<u8>, tag: &[u8; 4], body: &[u8]) {
    out.extend_from_slice(tag);
    write_varint(out, body.len() as u32);
    out.extend_from_slice(body);
}

fn write_string(out: &mut Vec<u8>, s: &str) {
    write_varint(out, s.len() as u32);
    out.extend_from_slice(s.as_bytes());
}

/// Encodes a [`Song`] to PSNG bytes, chunks in HEAD, ROWS, PATT..., ARRG
/// order with minimal varints.
///
/// Precondition: `song` passes [`Song::validate`]. This is infallible and
/// does not check that; out-of-range fields (e.g. a `vol` over 127) are
/// written as given and are not preserved as errors.
pub fn encode(song: &Song) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"PSNG");
    out.push(1u8);

    let mut head_body = Vec::new();
    write_varint(&mut head_body, UNITS_PER_BEAT);
    write_varint(&mut head_body, song.tempo);
    write_varint(&mut head_body, song.swing);
    write_varint(&mut head_body, zigzag_encode(song.key));
    head_body.push(song.voice_mask);
    write_chunk(&mut out, b"HEAD", &head_body);

    let mut rows_body = Vec::new();
    write_varint(&mut rows_body, song.rows.len() as u32);
    for row in &song.rows {
        write_string(&mut rows_body, &row.sound);
        write_varint(&mut rows_body, row.note.map_or(0, |n| n + 1));
        write_varint(&mut rows_body, row.vol);
        write_varint(&mut rows_body, zigzag_encode(row.pan));
    }
    write_chunk(&mut out, b"ROWS", &rows_body);

    for pat in &song.patterns {
        let mut body = Vec::new();
        write_string(&mut body, &pat.name);
        write_varint(&mut body, pat.length);
        write_varint(&mut body, pat.tempo.unwrap_or(0));
        write_varint(&mut body, pat.notes.len() as u32);
        let mut prev: i64 = 0;
        for note in &pat.notes {
            let delta = (note.at as i64 - prev) as i32;
            write_varint(&mut body, zigzag_encode(delta));
            prev = note.at as i64;
            let pinned = note.voice.is_some();
            let nudged = note.nudge != 0;
            let packed = (note.row << 2) | (pinned as u32) | ((nudged as u32) << 1);
            write_varint(&mut body, packed);
            write_varint(&mut body, note.len);
            write_varint(&mut body, note.vel);
            if let Some(v) = note.voice {
                body.push(v as u8);
            }
            if nudged {
                write_varint(&mut body, zigzag_encode(note.nudge));
            }
        }
        write_chunk(&mut out, b"PATT", &body);
    }

    let mut arrg_body = Vec::new();
    write_varint(&mut arrg_body, song.arrangement.len() as u32);
    for &idx in &song.arrangement {
        write_varint(&mut arrg_body, idx);
    }
    write_chunk(&mut out, b"ARRG", &arrg_body);

    out
}

impl Song {
    /// Static range checks: tempo/swing, rows, arrangement, and every
    /// pattern's length and notes. Does not check timing (a note's nudge
    /// pushing its start negative needs [`compile`]).
    pub fn validate(&self) -> Result<(), SongError> {
        if !TEMPO_RANGE.contains(&self.tempo) {
            return Err(SongError("tempo must be 1..400 BPM".into()));
        }
        if self.swing > 75 {
            return Err(SongError("swing must be 0..75".into()));
        }
        if self.rows.is_empty() {
            return Err(SongError("a song needs at least one row".into()));
        }
        for (i, row) in self.rows.iter().enumerate() {
            let n = i + 1;
            if row.vol > 127 {
                return Err(SongError(format!("row {n}: vol {} is over 127", row.vol)));
            }
            if !(-100..=100).contains(&row.pan) {
                return Err(SongError(format!("row {n}: pan must be -100..100")));
            }
            if let Some(note) = row.note {
                if note > 127 {
                    return Err(SongError(format!("row {n}: note {note} is over 127")));
                }
            }
        }
        if self.arrangement.is_empty() {
            return Err(SongError("the arrangement is empty".into()));
        }
        for (i, &p) in self.arrangement.iter().enumerate() {
            if p as usize >= self.patterns.len() {
                return Err(SongError(format!(
                    "arrangement slot {} names unknown pattern {p}",
                    i + 1
                )));
            }
        }
        // Every arrangement slot names a valid pattern (checked above): sum
        // the notes it plays with saturating u64 math, before compile ever
        // allocates an events Vec sized off untrusted input.
        let total_events: u64 = self
            .arrangement
            .iter()
            .map(|&p| self.patterns[p as usize].notes.len() as u64)
            .fold(0u64, |acc, n| acc.saturating_add(n));
        if total_events > MAX_EVENTS {
            return Err(SongError(format!("the song plays over {MAX_EVENTS} notes")));
        }
        for pat in &self.patterns {
            if let Some(t) = pat.tempo {
                if !TEMPO_RANGE.contains(&t) {
                    return Err(SongError(format!(
                        "pattern '{}': tempo must be 1..400 BPM",
                        pat.name
                    )));
                }
            }
            if pat.length < 1 {
                return Err(SongError(format!(
                    "pattern '{}': length must be at least 1",
                    pat.name
                )));
            }
            for (i, note) in pat.notes.iter().enumerate() {
                let n = i + 1;
                // Built lazily: only the branch that actually errors pays
                // for this format!, not every note.
                let where_ = || format!("pattern '{}' note {n}", pat.name);
                if note.row as usize >= self.rows.len() {
                    return Err(SongError(format!(
                        "{}: row {} does not exist",
                        where_(),
                        note.row + 1
                    )));
                }
                if note.vel > 127 {
                    return Err(SongError(format!(
                        "{}: vel {} is over 127",
                        where_(),
                        note.vel
                    )));
                }
                if note.len < 1 {
                    return Err(SongError(format!("{}: len must be at least 1", where_())));
                }
                if note.at >= pat.length {
                    return Err(SongError(format!(
                        "{}: starts past the pattern's end",
                        where_()
                    )));
                }
                match note.voice {
                    Some(v) => {
                        if v > 7 {
                            return Err(SongError(format!("{}: voice {v} is not 0..7", where_())));
                        }
                        if self.voice_mask & (1 << v) == 0 {
                            return Err(SongError(format!(
                                "{}: voice {v} is outside the song's voice mask",
                                where_()
                            )));
                        }
                    }
                    None => {
                        if self.voice_mask == 0 {
                            return Err(SongError(format!(
                                "{}: the song's voice mask is empty",
                                where_()
                            )));
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

// --- timing ------------------------------------------------------------

/// One arrangement slot's position, span and effective tempo, plus the
/// tick map of the run (maximal run of consecutive same-tempo slots) it
/// belongs to.
#[derive(Clone, Debug, PartialEq)]
pub struct SlotTiming {
    pub pattern: u32,
    /// Slot start, in units.
    pub pos: u64,
    pub len: u32,
    /// Effective tempo, BPM.
    pub bpm: f64,
    /// This run's first slot's `pos`.
    pub run_pos: u64,
    /// The tick at `run_pos`.
    pub run_tick: i64,
}

/// Global position <-> tick map for one arrangement.
#[derive(Debug)]
pub struct Timing {
    pub slots: Vec<SlotTiming>,
    /// Total units.
    pub end: u64,
    /// Ticks; equal to `tick(end)`.
    pub length: i64,
    swing: u32,
}

/// The shared start/tick formula: evaluates the given slot's timing at
/// `offset` units into it (its own `pos`, `run_pos`, `run_tick`, `len` and
/// `bpm`), applying swing if it's active and the offset's 16th pair fits
/// wholly inside the slot's pattern. Used both by `Timing::tick` (offset
/// into the slot containing a position) and by `Timing::new` (offset = a
/// slot's own `len`, to find the next run's starting tick).
fn tick_in_slot(s: &SlotTiming, offset: u64, swing: u32) -> i64 {
    let r = (s.pos - s.run_pos) + offset;
    let w = swing as f64 / 100.0;
    let pair_units = UNITS_PER_16TH * 2;
    let q = offset % pair_units;
    let pair_start = offset - q;
    let swing_ok = w > 0.0 && pair_start + pair_units <= s.len as u64;
    let u16th = UNITS_PER_16TH as f64;
    let steps = if swing_ok {
        if q < UNITS_PER_16TH {
            ((r - q) as f64 / u16th) + (q as f64 / u16th) * (1.0 + w)
        } else {
            ((r - q + UNITS_PER_16TH) as f64 / u16th)
                + w
                + ((q - UNITS_PER_16TH) as f64 / u16th) * (1.0 - w)
        }
    } else {
        r as f64 / u16th
    };
    // 3750 = 250 engine ticks/s * 60 s/min / 4 sixteenths/beat: ticks per
    // 16th note at 1 BPM.
    let step_ticks = 3750.0 / s.bpm;
    s.run_tick + (steps * step_ticks + 0.5).floor() as i64
}

impl Timing {
    /// Builds the slot/run map for `song`'s arrangement. Assumes
    /// `song.validate()` passed.
    pub(crate) fn new(song: &Song) -> Timing {
        let mut slots = Vec::with_capacity(song.arrangement.len());
        let mut pos: u64 = 0;
        let mut run_pos: u64 = 0;
        let mut run_tick: i64 = 0;
        let mut prev_centi: Option<u32> = None;

        for &pat_idx in &song.arrangement {
            let pat = &song.patterns[pat_idx as usize];
            let centi = pat.tempo.unwrap_or(song.tempo);
            if let Some(prev) = prev_centi {
                if prev != centi {
                    let prev_slot = slots.last().unwrap();
                    run_tick = tick_in_slot(prev_slot, prev_slot.len as u64, song.swing);
                    run_pos = pos;
                }
            }
            slots.push(SlotTiming {
                pattern: pat_idx,
                pos,
                len: pat.length,
                bpm: centi as f64 / 100.0,
                run_pos,
                run_tick,
            });
            pos += pat.length as u64;
            prev_centi = Some(centi);
        }

        let end = pos;
        let mut timing = Timing {
            slots,
            end,
            length: 0,
            swing: song.swing,
        };
        timing.length = timing.tick(end);
        timing
    }

    /// The slot containing `pos` (a boundary position belongs to the slot
    /// starting there; `pos == end` belongs to the last slot).
    pub fn slot_at(&self, pos: u64) -> usize {
        debug_assert!(!self.slots.is_empty(), "slot_at on an empty arrangement");
        if pos >= self.end {
            return self.slots.len() - 1;
        }
        match self.slots.binary_search_by(|s| s.pos.cmp(&pos)) {
            Ok(i) => i,
            Err(i) => i - 1,
        }
    }

    /// Maps a song position (`0..=end`) to its engine tick.
    pub fn tick(&self, pos: u64) -> i64 {
        let idx = self.slot_at(pos);
        let s = &self.slots[idx];
        let offset = pos - s.pos;
        tick_in_slot(s, offset, self.swing)
    }
}

// --- compile / allocate --------------------------------------------------

/// One playable event: a note placed at an absolute tick range on a voice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub start: i64,
    pub end: i64,
    /// Index into the song's rows.
    pub row: u32,
    /// The voice [`allocate`] assigned.
    pub voice: u8,
    /// The note's pin, if any (already validated to be inside the mask).
    pub pin: Option<u8>,
    pub l: u8,
    pub r: u8,
    /// Arrangement slot index this event came from.
    pub slot: u32,
    /// Index of the source note within its pattern.
    pub note: u32,
}

/// A song's flattened, voice-assigned playback data.
#[derive(Debug)]
pub struct Compiled {
    pub events: Vec<Event>,
    pub timing: Timing,
}

/// Validates, times, flattens and allocates a song's notes into events,
/// sorted by start (ties keep arrangement order).
pub fn compile(song: &Song) -> Result<Compiled, SongError> {
    song.validate()?;
    let timing = Timing::new(song);
    let mut events = Vec::new();

    for (slot_idx, &pat_idx) in song.arrangement.iter().enumerate() {
        let pat = &song.patterns[pat_idx as usize];
        let s = &timing.slots[slot_idx];
        for (note_idx, n) in pat.notes.iter().enumerate() {
            let pos = s.pos + n.at as u64;
            let start = timing.tick(pos) + n.nudge as i64;
            if start < 0 {
                return Err(SongError(format!(
                    "pattern '{}' note {}: nudge starts it before the song",
                    pat.name,
                    note_idx + 1
                )));
            }
            let end_pos = (pos + n.len as u64).min(timing.end);
            let end = timing.tick(end_pos).max(start);

            let row = &song.rows[n.row as usize];
            let pan = row.pan as f64 / 100.0;
            let x_l = row.vol as f64 * (1.0 - pan).min(1.0);
            let x_r = row.vol as f64 * (1.0 + pan).min(1.0);
            let l = ((x_l * n.vel as f64 / 127.0) + 0.5).floor() as u8;
            let r = ((x_r * n.vel as f64 / 127.0) + 0.5).floor() as u8;

            events.push(Event {
                start,
                end,
                row: n.row,
                voice: 0,
                pin: n.voice.map(|v| v as u8),
                l,
                r,
                slot: slot_idx as u32,
                note: note_idx as u32,
            });
        }
    }

    events.sort_by_key(|e| e.start); // stable: ties keep (slot, note) order
    allocate(&mut events, song.voice_mask);
    Ok(Compiled { events, timing })
}

/// Assigns each event (already start-ordered) to a voice in `mask`. A
/// pinned event takes its voice and cuts whatever was still sounding there;
/// an unpinned one takes the lowest free voice in the mask, or steals the
/// one whose current event started earliest (lowest voice on a tie),
/// cutting that event's end. A voice outside `mask` is never chosen.
pub(crate) fn allocate(events: &mut [Event], mask: u8) {
    let mut last: [Option<usize>; 8] = [None; 8];

    for i in 0..events.len() {
        let start = events[i].start;
        let voice = if let Some(pv) = events[i].pin {
            if let Some(li) = last[pv as usize] {
                if events[li].end > start {
                    events[li].end = start;
                }
            }
            pv
        } else {
            let free = (0..8u8).find(|&v| {
                mask & (1 << v) != 0 && last[v as usize].is_none_or(|li| events[li].end <= start)
            });
            match free {
                Some(v) => v,
                None => {
                    let steal = (0..8u8)
                        .filter(|&v| mask & (1 << v) != 0 && last[v as usize].is_some())
                        .min_by_key(|&v| events[last[v as usize].unwrap()].start);
                    match steal {
                        Some(v) => {
                            let li = last[v as usize].unwrap();
                            events[li].end = start;
                            v
                        }
                        None => 0, // empty mask: validate() would already have errored
                    }
                }
            }
        };
        events[i].voice = voice;
        last[voice as usize] = Some(i);
    }
}
