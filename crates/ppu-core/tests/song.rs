//! `ppu_core::song`: the PSNG codec, timing map, compiler and voice
//! allocator, exercised entirely through the public API.

mod common;

use ppu_core::song::*;
use ppu_core::LuaEngine;
use std::time::{Duration, Instant};

/// A song touching every field: notes with and without a pitch, negative
/// pan/key, both forms of pattern tempo, pinned and nudged notes (including
/// a negative nudge), and notes stored out of `at` order.
fn sample_song() -> Song {
    Song {
        tempo: 13750,
        swing: 42,
        key: -7,
        voice_mask: 0b1010_1101,
        rows: vec![
            Row {
                sound: "kick".into(),
                note: None,
                vol: 127,
                pan: -80,
            },
            Row {
                sound: "snare".into(),
                note: Some(64),
                vol: 100,
                pan: 25,
            },
        ],
        patterns: vec![
            Pattern {
                name: "verse".into(),
                length: 48,
                tempo: None,
                notes: vec![
                    // Stored out of `at` order: this note's `at` (24) is
                    // greater than the next one's (0).
                    Note {
                        at: 24,
                        row: 0,
                        len: 12,
                        vel: 100,
                        voice: None,
                        nudge: 0,
                        end_nudge: 0,
                    },
                    Note {
                        at: 0,
                        row: 1,
                        len: 6,
                        vel: 64,
                        voice: Some(3),
                        nudge: -5,
                        end_nudge: 0,
                    },
                ],
            },
            Pattern {
                name: "chorus".into(),
                length: 96,
                tempo: Some(9000),
                notes: vec![
                    Note {
                        at: 10,
                        row: 0,
                        len: 20,
                        vel: 127,
                        voice: Some(7),
                        nudge: 9,
                        end_nudge: 0,
                    },
                    Note {
                        at: 50,
                        row: 1,
                        len: 1,
                        vel: 1,
                        voice: None,
                        nudge: 0,
                        end_nudge: 0,
                    },
                ],
            },
        ],
        arrangement: vec![0, 1, 0],
        loop_start: 0,
    }
}

/// A minimal, valid song (one row, one pattern, one arrangement slot), for
/// tests that mutate a single field to break one validation rule.
fn valid_song() -> Song {
    Song {
        tempo: 12000,
        swing: 0,
        key: 0,
        voice_mask: 0b0000_0001,
        rows: vec![Row {
            sound: "kick".into(),
            note: None,
            vol: 100,
            pan: 0,
        }],
        patterns: vec![Pattern {
            name: "A".into(),
            length: 12,
            tempo: None,
            notes: vec![Note {
                at: 0,
                row: 0,
                len: 12,
                vel: 100,
                voice: None,
                nudge: 0,
                end_nudge: 0,
            }],
        }],
        arrangement: vec![0],
        loop_start: 0,
    }
}

// ---- 1. round trip ---------------------------------------------------

/// Every field survives encode -> decode, and re-encoding the decoded song
/// reproduces the exact same bytes.
#[test]
fn round_trip_preserves_every_field() {
    let song = sample_song();
    let bytes = encode(&song);
    let decoded = decode(&bytes).expect("decode");
    assert_eq!(decoded, song);
    assert_eq!(encode(&decoded), bytes);
}

// ---- 2. unknown chunk skipped -----------------------------------------

fn xtra_chunk() -> Vec<u8> {
    let mut c = Vec::new();
    c.extend_from_slice(b"XTRA");
    c.push(3); // varint body length (fits in one byte)
    c.extend_from_slice(&[1, 2, 3]);
    c
}

/// An unrecognized chunk tag, spliced between HEAD and ROWS and again at
/// the end of the file, is skipped rather than rejected.
#[test]
fn unknown_chunk_is_skipped() {
    let song = sample_song();
    let bytes = encode(&song);
    let rows_pos = bytes
        .windows(4)
        .position(|w| w == b"ROWS")
        .expect("ROWS chunk present");

    let mut spliced = bytes[..rows_pos].to_vec();
    spliced.extend(xtra_chunk());
    spliced.extend_from_slice(&bytes[rows_pos..]);
    spliced.extend(xtra_chunk());

    let decoded = decode(&spliced).expect("decode with unknown chunks");
    assert_eq!(decoded, song);
}

// ---- 3. truncation / malformation names the chunk ----------------------

/// Cutting the input inside the second PATT chunk's body names that
/// specific pattern by number.
#[test]
fn truncated_patt_body_names_the_pattern_number() {
    let bytes = encode(&sample_song());
    let second_patt = bytes
        .windows(4)
        .enumerate()
        .filter(|(_, w)| *w == b"PATT")
        .nth(1)
        .map(|(i, _)| i)
        .expect("two PATT chunks");
    // tag(4) + length byte(1) + a few body bytes, well short of the full body.
    let cut = second_patt + 8;
    assert!(
        cut < bytes.len(),
        "test song's 2nd PATT body must exceed 3 bytes"
    );
    let err = decode(&bytes[..cut]).unwrap_err();
    assert!(err.0.contains("PATT chunk 2 truncated"), "{}", err.0);
}

/// Cutting the input inside the HEAD chunk's body names it too.
#[test]
fn truncated_head_body_names_head() {
    let bytes = encode(&sample_song());
    // "PSNG" + version(1) + tag(4) + length byte(1) + 2 body bytes.
    let cut = 5 + 4 + 1 + 2;
    let err = decode(&bytes[..cut]).unwrap_err();
    assert!(err.0.contains("HEAD chunk truncated"), "{}", err.0);
}

/// A file that doesn't start with the PSNG magic is rejected outright.
#[test]
fn bad_magic_is_rejected() {
    let mut bytes = encode(&sample_song());
    bytes[0] = b'X';
    let err = decode(&bytes).unwrap_err();
    assert_eq!(err.0, "not a PSNG song");
}

/// A version other than 1 or 2 is rejected by number.
#[test]
fn unsupported_version_is_rejected() {
    let mut bytes = encode(&sample_song());
    bytes[4] = 3;
    let err = decode(&bytes).unwrap_err();
    assert_eq!(err.0, "unsupported PSNG version 3");
}

/// Dropping the ARRG chunk entirely is a distinct error from truncation.
#[test]
fn missing_arrg_is_rejected() {
    let bytes = encode(&sample_song());
    let arrg_pos = bytes
        .windows(4)
        .position(|w| w == b"ARRG")
        .expect("ARRG present");
    let err = decode(&bytes[..arrg_pos]).unwrap_err();
    assert_eq!(err.0, "missing ARRG chunk");
}

/// A HEAD chunk appearing twice is rejected, not silently overwritten.
#[test]
fn duplicate_head_is_rejected() {
    let bytes = encode(&sample_song());
    let rows_pos = bytes
        .windows(4)
        .position(|w| w == b"ROWS")
        .expect("ROWS present");
    let head_chunk = bytes[5..rows_pos].to_vec();
    let mut dup = bytes[..rows_pos].to_vec();
    dup.extend_from_slice(&head_chunk);
    dup.extend_from_slice(&bytes[rows_pos..]);
    let err = decode(&dup).unwrap_err();
    assert_eq!(err.0, "duplicate HEAD chunk");
}

/// A chunk whose declared body length overruns what it actually contains
/// (an unconsumed byte left over) is a distinct "trailing bytes" error.
#[test]
fn trailing_bytes_in_a_body_is_rejected() {
    let bytes = encode(&sample_song());
    let arrg_pos = bytes
        .windows(4)
        .position(|w| w == b"ARRG")
        .expect("ARRG present");
    let len_byte_pos = arrg_pos + 4;
    assert!(
        bytes[len_byte_pos] < 127,
        "ARRG body length must fit in one varint byte"
    );
    let mut mutated = bytes.clone();
    mutated[len_byte_pos] += 1; // declare one extra body byte...
    mutated.push(0xaa); // ...and supply it (ARRG is the last chunk).
    let err = decode(&mutated).unwrap_err();
    assert_eq!(err.0, "ARRG chunk has trailing bytes");
}

/// Builds one raw chunk: a 4-byte tag, a one-byte varint body length (the
/// bodies these tests use always fit in one byte), and the body.
fn build_chunk(tag: &[u8; 4], body: &[u8]) -> Vec<u8> {
    assert!(body.len() < 128, "test body must fit a one-byte varint");
    let mut out = Vec::new();
    out.extend_from_slice(tag);
    out.push(body.len() as u8);
    out.extend_from_slice(body);
    out
}

/// A `u32::MAX` element count, LEB128-encoded, on its own -- exactly what a
/// hostile ROWS/PATT-notes/ARRG count would look like ahead of a body far
/// too short to hold that many elements.
const HUGE_COUNT_VARINT: [u8; 5] = [0xff, 0xff, 0xff, 0xff, 0x0f];

/// A ROWS chunk declaring `u32::MAX` rows but supplying none must not size
/// an allocation off that raw count (which would abort the process); it
/// must fail gracefully once the short body runs out.
#[test]
fn rows_chunk_with_huge_count_and_short_body_is_truncated_not_aborted() {
    let mut bytes = b"PSNG".to_vec();
    bytes.push(1);
    bytes.extend(build_chunk(b"ROWS", &HUGE_COUNT_VARINT));
    let err = decode(&bytes).unwrap_err();
    assert!(err.0.contains("ROWS chunk truncated"), "{}", err.0);
}

/// Same for a PATT chunk's note count.
#[test]
fn patt_chunk_with_huge_note_count_and_short_body_is_truncated_not_aborted() {
    // name "" (varint len 0), length 4, tempo 0 (none), then the huge note count.
    let mut body = vec![0x00, 0x04, 0x00];
    body.extend_from_slice(&HUGE_COUNT_VARINT);
    let mut bytes = b"PSNG".to_vec();
    bytes.push(1);
    bytes.extend(build_chunk(b"PATT", &body));
    let err = decode(&bytes).unwrap_err();
    assert!(err.0.contains("PATT chunk 1 truncated"), "{}", err.0);
}

/// Same for an ARRG chunk's slot count.
#[test]
fn arrg_chunk_with_huge_count_and_short_body_is_truncated_not_aborted() {
    let mut bytes = b"PSNG".to_vec();
    bytes.push(1);
    bytes.extend(build_chunk(b"ARRG", &HUGE_COUNT_VARINT));
    let err = decode(&bytes).unwrap_err();
    assert!(err.0.contains("ARRG chunk truncated"), "{}", err.0);
}

/// A chunk header declaring a `u32::MAX` body length, with no such body
/// present, is a truncation error, not a wrapped/underflowed bounds check
/// that reads out of range. (On this host's 64-bit `usize` the old
/// `pos + body_len > bytes.len()` already happened not to wrap -- this test
/// pins the contract, and the fix matters on 32-bit targets such as wasm32,
/// where `body_len` alone can equal `usize::MAX`.)
#[test]
fn chunk_header_with_huge_body_length_is_truncated_not_a_panic() {
    let mut bytes = b"PSNG".to_vec();
    bytes.push(1);
    bytes.extend_from_slice(b"ROWS");
    bytes.extend_from_slice(&HUGE_COUNT_VARINT);
    let err = decode(&bytes).unwrap_err();
    assert!(err.0.contains("ROWS chunk truncated"), "{}", err.0);
}

/// A row's `sound` string declaring a `u32::MAX` byte length, with no such
/// body present, is a truncation error for the same reason.
#[test]
fn string_with_huge_length_is_truncated_not_a_panic() {
    let mut body = vec![0x01]; // ROWS count = 1
    body.extend_from_slice(&HUGE_COUNT_VARINT); // sound string length
    let mut bytes = b"PSNG".to_vec();
    bytes.push(1);
    bytes.extend(build_chunk(b"ROWS", &body));
    let err = decode(&bytes).unwrap_err();
    assert!(err.0.contains("ROWS chunk truncated"), "{}", err.0);
}

// ---- 4. validation messages --------------------------------------------

#[test]
fn validate_rejects_row_field_out_of_range() {
    let mut song = valid_song();
    song.rows[0].vol = 200;
    let err = song.validate().unwrap_err();
    assert!(
        err.0.contains("row 1") && err.0.contains("127"),
        "{}",
        err.0
    );
}

#[test]
fn validate_rejects_note_vel_over_127() {
    let mut song = valid_song();
    song.patterns[0].notes[0].vel = 200;
    let err = song.validate().unwrap_err();
    assert_eq!(err.0, "pattern 'A' note 1: vel 200 is over 127");
}

#[test]
fn validate_rejects_note_len_under_1() {
    let mut song = valid_song();
    song.patterns[0].notes[0].len = 0;
    let err = song.validate().unwrap_err();
    assert_eq!(err.0, "pattern 'A' note 1: len must be at least 1");
}

#[test]
fn validate_rejects_pinned_voice_over_7() {
    let mut song = valid_song();
    song.patterns[0].notes[0].voice = Some(9);
    let err = song.validate().unwrap_err();
    assert_eq!(err.0, "pattern 'A' note 1: voice 9 is not 0..7");
}

#[test]
fn validate_rejects_pinned_voice_outside_mask() {
    let mut song = valid_song(); // mask = 0b0000_0001, only voice 0
    song.patterns[0].notes[0].voice = Some(1);
    let err = song.validate().unwrap_err();
    assert_eq!(
        err.0,
        "pattern 'A' note 1: voice 1 is outside the song's voice mask"
    );
}

#[test]
fn validate_rejects_unknown_pattern_in_arrangement() {
    let mut song = valid_song();
    song.arrangement = vec![5];
    let err = song.validate().unwrap_err();
    assert_eq!(err.0, "arrangement slot 1 names unknown pattern 5");
}

/// A note's row error names the row the same way the row-field errors do:
/// 1-based (index 9 -> "row 10", not "row 9").
#[test]
fn validate_rejects_unknown_row_message_is_one_based() {
    let mut song = valid_song(); // one row, at index 0
    song.patterns[0].notes[0].row = 9;
    let err = song.validate().unwrap_err();
    assert_eq!(err.0, "pattern 'A' note 1: row 10 does not exist");
}

#[test]
fn validate_rejects_tempo_out_of_range() {
    let mut song = valid_song();
    song.tempo = 99;
    let err = song.validate().unwrap_err();
    assert_eq!(err.0, "tempo must be 1..400 BPM");
}

/// `validate()` alone doesn't know about timing; only `compile()` can tell
/// a large negative nudge pushes a note's start before the song begins.
#[test]
fn compile_rejects_nudge_before_the_song_starts() {
    let mut song = valid_song();
    song.patterns[0].notes[0].nudge = -1_000_000;
    assert!(song.validate().is_ok(), "validate() has no timing to check");
    let err = compile(&song).unwrap_err();
    assert_eq!(err.0, "pattern 'A' note 1: nudge starts it before the song");
}

/// `validate()` caps the total notes an arrangement can play (summed per
/// slot over its pattern's note count) at 1,000,000, computed before
/// `compile()` ever allocates an events `Vec` off that count.
#[test]
fn validate_rejects_a_song_playing_over_a_million_notes() {
    let notes: Vec<Note> = (0..1000u32).map(|i| note_at(i, 1, None)).collect();
    let mut song = tick_song(0b1, 1000, notes); // 1 pattern of 1000 notes
    song.arrangement = vec![0; 1001]; // * 1001 slots = 1,001,000 > MAX_EVENTS
    let err = song.validate().unwrap_err();
    assert_eq!(err.0, "the song plays over 1000000 notes");
}

// ---- 4b. end nudge ------------------------------------------------------

/// A nudged end can land at a tick between two unit ticks, not on either: at
/// 1 BPM (centi-tempo 100) one unit is 3750/12 = 312.5 ticks, so a unit
/// boundary can still land on a whole tick (e.g. tick(4) = 1250), but
/// `end_nudge` shifts the end by a plain tick count, free of that grid. Here
/// `end_nudge = -100` puts the end at 1463, strictly between tick(4) = 1250
/// and tick(5) = 1563.
#[test]
fn end_nudge_lands_between_two_unit_ticks_at_a_slow_tempo() {
    let mut song = valid_song(); // pattern "A", length 12
    song.tempo = 100; // 1 BPM
    song.patterns[0].notes[0] = Note {
        at: 0,
        row: 0,
        len: 5,
        vel: 100,
        voice: None,
        nudge: 0,
        end_nudge: -100,
    };
    let compiled = compile(&song).expect("compiles");
    // tick(r) = floor(r * 3750/12 + 0.5) = floor(r * 312.5 + 0.5).
    // start = tick(0) = floor(0 * 312.5 + 0.5) = floor(0.5) = 0.
    // unnudged end = tick(5) = floor(5 * 312.5 + 0.5) = floor(1562.5 + 0.5)
    //              = floor(1563.0) = 1563.
    // end = 1563 + end_nudge(-100) = 1463.
    let ev = &compiled.events[0];
    assert_eq!(ev.start, 0);
    assert_eq!(ev.end, 1463);
    assert_eq!(ev.want_end, 1463);
}

/// A start nudge and an end nudge on the same note both apply, independently.
#[test]
fn start_and_end_nudge_both_apply_on_the_same_note() {
    let mut song = valid_song(); // 120 BPM, pattern "A" length 12
    song.patterns[0].notes[0] = Note {
        at: 0,
        row: 0,
        len: 12,
        vel: 100,
        voice: None,
        nudge: 5,
        end_nudge: -7,
    };
    let compiled = compile(&song).expect("compiles");
    // At 120 BPM, step_ticks = 31.25: tick(0) = floor(0.5) = 0,
    // tick(12) = floor(1 * 31.25 + 0.5) = floor(31.75) = 31.
    // start = tick(0) + nudge(5) = 5.
    // end = tick(12) + end_nudge(-7) = 31 - 7 = 24.
    let ev = &compiled.events[0];
    assert_eq!(ev.start, 5);
    assert_eq!(ev.end, 24);
    assert_eq!(ev.want_end, 24);
}

/// An end nudge (or a start nudge with none) that puts a note's end at or
/// before its own start is refused, naming the pattern and note.
#[test]
fn compile_rejects_an_end_at_or_before_its_start() {
    let msg = "pattern 'A' note 1: ends at or before its start";
    // valid_song(): 120 BPM, note at 0, len 12 -> start = 0, unnudged end =
    // tick(12) = 31 (see start_and_end_nudge_both_apply_on_the_same_note).

    let mut at_start = valid_song();
    at_start.patterns[0].notes[0].end_nudge = -31; // end = 31 - 31 = 0 == start
    assert_eq!(compile(&at_start).unwrap_err().0, msg);

    let mut before_start = valid_song();
    before_start.patterns[0].notes[0].end_nudge = -32; // end = -1 < start
    assert_eq!(compile(&before_start).unwrap_err().0, msg);

    let mut nudge_past_end = valid_song();
    nudge_past_end.patterns[0].notes[0].nudge = 32; // start = 32 > unnudged end (31)
    assert_eq!(compile(&nudge_past_end).unwrap_err().0, msg);
}

/// Every one of the 8 pinned x nudged x end-nudged flag combinations,
/// including negative and positive end nudges and vel 127 on the
/// all-flags-set note, round-trips through encode/decode, and re-encoding
/// the decoded song reproduces the exact same bytes.
#[test]
fn round_trips_every_pin_nudge_end_nudge_flag_combination() {
    let notes: Vec<Note> = (0u32..8)
        .map(|i| {
            let pinned = i & 1 != 0;
            let nudged = i & 2 != 0;
            let end_nudged = i & 4 != 0;
            Note {
                at: i * 12,
                row: 0,
                len: 6,
                vel: if pinned && nudged && end_nudged {
                    127
                } else {
                    40 + i
                },
                voice: if pinned { Some(i % 8) } else { None },
                nudge: if nudged { -3 - i as i32 } else { 0 },
                end_nudge: if !end_nudged {
                    0
                } else if i % 2 == 0 {
                    -5 - i as i32
                } else {
                    5 + i as i32
                },
            }
        })
        .collect();
    let mut song = valid_song();
    song.patterns[0].length = 8 * 12;
    song.patterns[0].notes = notes;

    let bytes = encode(&song);
    let decoded = decode(&bytes).expect("decode");
    assert_eq!(decoded, song);
    assert_eq!(encode(&decoded), bytes);
}

fn fixture_song_without_end_nudges() -> Song {
    Song {
        tempo: 12000,
        swing: 10,
        key: 3,
        voice_mask: 0b0000_0011,
        rows: vec![Row {
            sound: "kick".into(),
            note: None,
            vol: 100,
            pan: 0,
        }],
        patterns: vec![Pattern {
            name: "A".into(),
            length: 24,
            tempo: None,
            notes: vec![
                Note {
                    at: 0,
                    row: 0,
                    len: 12,
                    vel: 100,
                    voice: Some(1),
                    nudge: 0,
                    end_nudge: 0,
                },
                Note {
                    at: 12,
                    row: 0,
                    len: 12,
                    vel: 50,
                    voice: None,
                    nudge: 5,
                    end_nudge: 0,
                },
            ],
        }],
        arrangement: vec![0],
        loop_start: 0,
    }
}

/// PSNG bytes of a song without end nudges, for `fixture_song_without_end_nudges`;
/// the format must not change for such songs.
const FIXTURE_BYTES_WITHOUT_END_NUDGES: &[u8] = &[
    80, 83, 78, 71, 1, 72, 69, 65, 68, 6, 48, 224, 93, 10, 6, 3, 82, 79, 87, 83, 9, 1, 4, 107, 105,
    99, 107, 0, 100, 0, 80, 65, 84, 84, 15, 1, 65, 24, 0, 2, 0, 1, 12, 100, 1, 24, 2, 12, 50, 10,
    65, 82, 82, 71, 2, 1, 0,
];

/// A song built with pinned and nudged notes but no end nudges (a literal
/// `Vec<u8>` captured from `encode()` on the base commit, before end nudges
/// existed) still encodes byte-identically, and decoding it re-encodes to
/// the same bytes.
#[test]
fn songs_without_end_nudges_encode_byte_identically_to_before() {
    let song = fixture_song_without_end_nudges();
    assert_eq!(encode(&song), FIXTURE_BYTES_WITHOUT_END_NUDGES);
    let decoded = decode(FIXTURE_BYTES_WITHOUT_END_NUDGES).expect("decode");
    assert_eq!(encode(&decoded), FIXTURE_BYTES_WITHOUT_END_NUDGES);
}

// ---- 5. kit parity of timing --------------------------------------------

/// tempo 120 / swing 30%, a 16-sixteenth pattern played twice: every
/// event's start/end must land on the same tick as today's kit.lua
/// `at(i) = floor((i [+ 0.3 if i odd]) * 3750/tempo + 0.5)`, computed here
/// independently of the module under test.
#[test]
fn timing_matches_kit_lua_16th_grid_with_swing() {
    let notes: Vec<Note> = (0..16u32)
        .map(|i| Note {
            at: i * 12,
            row: 0,
            len: 12,
            vel: 100,
            voice: None,
            nudge: 0,
            end_nudge: 0,
        })
        .collect();
    let song = Song {
        tempo: 12000,
        swing: 30,
        key: 0,
        voice_mask: 0b1,
        rows: vec![Row {
            sound: "kick".into(),
            note: None,
            vol: 100,
            pan: 0,
        }],
        patterns: vec![Pattern {
            name: "A".into(),
            length: 192,
            tempo: None,
            notes,
        }],
        arrangement: vec![0, 0],
        loop_start: 0,
    };
    let compiled = compile(&song).expect("compiles");

    let step_ticks = 3750.0_f64 / 120.0;
    let w = 0.30_f64;
    let at = |i: u32| -> i64 {
        let mut x = i as f64;
        if i % 2 == 1 {
            x += w;
        }
        (x * step_ticks + 0.5).floor() as i64
    };

    assert_eq!(compiled.events.len(), 32);
    for (i, e) in compiled.events.iter().enumerate() {
        let gi = i as u32; // events are start-sorted, identical here to the global 16th index
        assert_eq!(e.start, at(gi), "start mismatch at 16th {gi}");
        assert_eq!(e.end, at(gi + 1), "end mismatch at 16th {gi}");
    }
    assert_eq!(compiled.timing.length, at(32));
}

// ---- 6. the hand-computed acceptance test -------------------------------

/// tempo 120 (step_ticks 31.25), swing 0. Pattern A (length 36, three
/// 16ths) has a short tap at unit 0 and a long-sustain note at unit 24
/// (len 10000, so it always clamps to the song's end -- this is what
/// forces the voice contention below). Pattern B (length 48, its own
/// tempo 60 -> step_ticks 62.5) has a note pinned to voice 2, one nudged
/// +3, and one nudged -2. Arrangement [A, A, B, A], mask {0,1,2} (0b111).
/// Row 0: vol 127, pan -50 (-0.5) -- checks l/r rounding.
///
/// Slot layout (units): A@0 len36, A@36 len36, B@72 len48, A@120 len36,
/// end=156.
/// Runs: [A,A] run0 (run_pos 0, run_tick 0); B run1 starts at the tick
/// A's second slot ends: r=(36-0)+36=72, steps=72/12=6,
/// tick=floor(6*31.25+0.5)=floor(188.0)=188 -> run1 run_pos=72 run_tick=188;
/// A run2 starts where B ends: r=(72-72)+48=48, steps=48/12=4,
/// tick=188+floor(4*62.5+0.5)=188+250=438 -> run2 run_pos=120 run_tick=438.
/// timing.length = tick(156): offset=36 in run2, r=36, steps=3,
/// tick=438+floor(3*31.25+0.5)=438+94=532.
///
/// l/r for row (vol127, pan-0.5): x_l=127*min(1,1.5)=127, x_r=127*min(1,0.5)=63.5.
/// vel100 -> l=floor(127*100/127+0.5)=100, r=floor(63.5*100/127+0.5)=50.
/// vel127 -> l=127, r=floor(63.5*127/127+0.5)=64.
/// vel64  -> l=64,  r=floor(63.5*64/127+0.5)=32.
/// vel90  -> l=90,  r=floor(63.5*90/127+0.5)=45.
/// vel50  -> l=50,  r=floor(63.5*50/127+0.5)=25.
#[test]
fn hand_computed_mixed_tempo_pinned_nudged_song() {
    let row = Row {
        sound: "kick".into(),
        note: None,
        vol: 127,
        pan: -50,
    };
    let pattern_a = Pattern {
        name: "A".into(),
        length: 36,
        tempo: None,
        notes: vec![
            Note {
                at: 0,
                row: 0,
                len: 12,
                vel: 100,
                voice: None,
                nudge: 0,
                end_nudge: 0,
            },
            // len 10000 always clamps to the song's end -> forces overlap.
            Note {
                at: 24,
                row: 0,
                len: 10000,
                vel: 127,
                voice: None,
                nudge: 0,
                end_nudge: 0,
            },
        ],
    };
    let pattern_b = Pattern {
        name: "B".into(),
        length: 48,
        tempo: Some(6000), // 60 BPM
        notes: vec![
            Note {
                at: 0,
                row: 0,
                len: 12,
                vel: 64,
                voice: Some(2),
                nudge: 0,
                end_nudge: 0,
            },
            Note {
                at: 12,
                row: 0,
                len: 12,
                vel: 90,
                voice: None,
                nudge: 3,
                end_nudge: 0,
            },
            Note {
                at: 24,
                row: 0,
                len: 12,
                vel: 50,
                voice: None,
                nudge: -2,
                end_nudge: 0,
            },
        ],
    };
    let song = Song {
        tempo: 12000, // 120 BPM
        swing: 0,
        key: 0,
        voice_mask: 0b0000_0111,
        rows: vec![row],
        patterns: vec![pattern_a, pattern_b],
        arrangement: vec![0, 0, 1, 0],
        loop_start: 0,
    };

    let compiled = compile(&song).expect("compiles");

    // "no drift": slot1 (second A) starts at 3 sixteenths into the run:
    // floor(3*31.25+0.5) = floor(94.25) = 94.
    assert_eq!(compiled.timing.tick(36), 94);
    // The last A starts exactly where B's run ends.
    assert_eq!(compiled.timing.tick(120), 438);
    assert_eq!(compiled.timing.length, 532);

    let ev = |start, end, voice, pin, l, r, slot, note| Event {
        start,
        end,
        want_end: end,
        row: 0,
        voice,
        pin,
        l,
        r,
        slot,
        note,
    };
    let mut expected = vec![
        // slot0 (A @ pos0, run0)
        ev(0, 31, 0, None, 100, 50, 0, 0), // start floor(0*31.25+0.5)=0; end tick(12)=floor(1*31.25+0.5)=31
        ev(63, 311, 0, None, 127, 64, 0, 1), // start tick(24)=floor(2*31.25+0.5)=63; end clamps to song end(532), then CUT to 311 by the steal below
        // slot1 (A @ pos36, still run0)
        ev(94, 125, 1, None, 100, 50, 1, 0), // start=tick(36)=94; end=tick(48)=floor(4*31.25+0.5)=125
        ev(156, 532, 1, None, 127, 64, 1, 1), // start=tick(60)=floor(5*31.25+0.5)=156; end clamps to song end=532
        // slot2 (B @ pos72, run1, tempo 60)
        ev(188, 251, 2, Some(2), 64, 32, 2, 0), // start=tick(72)=188 (run1 start); end=tick(84)=188+floor(1*62.5+0.5)=251
        ev(254, 313, 2, None, 90, 45, 2, 1), // start=tick(84)+3=254; end=tick(96)=188+floor(2*62.5+0.5)=313
        // stolen from voice0 (E1 above), whose start (63) is the earliest of {63,156,254}
        ev(311, 376, 0, None, 50, 25, 2, 2), // start=tick(96)-2=311; end=tick(108)=188+floor(3*62.5+0.5)=376
        // slot3 (A @ pos120, run2)
        ev(438, 469, 0, None, 100, 50, 3, 0), // start=tick(120)=438; end=tick(132)=438+floor(1*31.25+0.5)=469
        ev(501, 532, 0, None, 127, 64, 3, 1), // start=tick(144)=438+floor(2*31.25+0.5)=501; end clamps to song end=532
    ];
    expected[1].want_end = 532; // the stolen note still wanted the song's end
    assert_eq!(compiled.events, expected);
}

// ---- 6b. more swing coverage --------------------------------------------

/// Same kit-lua parity check as `timing_matches_kit_lua_16th_grid_with_swing`
/// above, but at an inexact tempo (60.24 BPM) and over 600 consecutive
/// swung 16ths, to rule out accumulated rounding drift the round-tempo,
/// 32-note case could hide.
#[test]
fn timing_matches_kit_lua_16th_grid_with_swing_at_an_inexact_tempo() {
    const N: u32 = 600;
    let notes: Vec<Note> = (0..N)
        .map(|i| Note {
            at: i * 12,
            row: 0,
            len: 12,
            vel: 100,
            voice: None,
            nudge: 0,
            end_nudge: 0,
        })
        .collect();
    let song = Song {
        tempo: 6024, // 60.24 BPM
        swing: 30,
        key: 0,
        voice_mask: 0b1,
        rows: vec![Row {
            sound: "kick".into(),
            note: None,
            vol: 100,
            pan: 0,
        }],
        patterns: vec![Pattern {
            name: "A".into(),
            length: N * 12,
            tempo: None,
            notes,
        }],
        arrangement: vec![0],
        loop_start: 0,
    };
    let compiled = compile(&song).expect("compiles");

    let step_ticks = 3750.0_f64 / 60.24;
    let w = 0.30_f64;
    let at = |i: u32| -> i64 {
        let mut x = i as f64;
        if i % 2 == 1 {
            x += w;
        }
        (x * step_ticks + 0.5).floor() as i64
    };

    assert_eq!(compiled.events.len(), N as usize);
    for (i, e) in compiled.events.iter().enumerate() {
        let gi = i as u32;
        assert_eq!(e.start, at(gi), "start mismatch at 16th {gi}");
        assert_eq!(e.end, at(gi + 1), "end mismatch at 16th {gi}");
    }
    assert_eq!(compiled.timing.length, at(N));
}

/// A swing pair whose phase lands on a 32nd (not a 16th boundary) still
/// warps correctly. Swing 50%, tempo 120 (step_ticks 31.25), pattern length
/// 48. Note at unit 6 (q=6 < 12): steps = 0 + 0.5*1.5 = 0.75 ->
/// floor(0.75*31.25+0.5) = floor(23.9375) = 23. Note at unit 18 (q=18 >=
/// 12): steps = 1 + 0.5 + 0.5*0.5 = 1.75 -> floor(1.75*31.25+0.5) =
/// floor(55.1875) = 55.
#[test]
fn swing_warps_an_off_16th_pair_correctly() {
    let song = Song {
        tempo: 12000,
        swing: 50,
        key: 0,
        voice_mask: 0b1,
        rows: vec![Row {
            sound: "kick".into(),
            note: None,
            vol: 100,
            pan: 0,
        }],
        patterns: vec![Pattern {
            name: "A".into(),
            length: 48,
            tempo: None,
            notes: vec![
                Note {
                    at: 6,
                    row: 0,
                    len: 1,
                    vel: 100,
                    voice: None,
                    nudge: 0,
                    end_nudge: 0,
                },
                Note {
                    at: 18,
                    row: 0,
                    len: 1,
                    vel: 100,
                    voice: None,
                    nudge: 0,
                    end_nudge: 0,
                },
            ],
        }],
        arrangement: vec![0],
        loop_start: 0,
    };
    let compiled = compile(&song).expect("compiles");
    assert_eq!(compiled.events[0].start, 23);
    assert_eq!(compiled.events[1].start, 55);
}

/// In an odd-length (36-unit) pattern with swing, only a 16th pair that
/// lies wholly inside the pattern is warped. Swing 50%, tempo 120
/// (step_ticks 31.25). The pair starting at 0 (units 0..24) fits, so the
/// note at unit 12 (q=12, mid-pair) IS swung: steps = 1 + 0.5 + 0 = 1.5 ->
/// floor(1.5*31.25+0.5) = floor(47.375) = 47. The pair starting at 24
/// (units 24..48) does NOT fit (48 > 36), so the note at unit 30 is NOT
/// swung: steps = 30/12 = 2.5 -> floor(2.5*31.25+0.5) = floor(78.625) = 78.
/// In arrangement [odd, odd], the second slot's note at unit 12 uses the
/// same pattern-relative phase (q=12, its own pair still fits) but the
/// run's own offset r=48: steps = (48-12+12)/12 + 0.5 + 0 = 4.5 ->
/// floor(4.5*31.25+0.5) = floor(141.125) = 141.
#[test]
fn swing_only_warps_a_pair_wholly_inside_an_odd_length_pattern() {
    let pattern = Pattern {
        name: "odd".into(),
        length: 36,
        tempo: None,
        notes: vec![
            Note {
                at: 12,
                row: 0,
                len: 1,
                vel: 100,
                voice: None,
                nudge: 0,
                end_nudge: 0,
            },
            Note {
                at: 30,
                row: 0,
                len: 1,
                vel: 100,
                voice: None,
                nudge: 0,
                end_nudge: 0,
            },
        ],
    };
    let row = Row {
        sound: "kick".into(),
        note: None,
        vol: 100,
        pan: 0,
    };

    let single = Song {
        tempo: 12000,
        swing: 50,
        key: 0,
        voice_mask: 0b1,
        rows: vec![row],
        patterns: vec![pattern],
        arrangement: vec![0],
        loop_start: 0,
    };
    let compiled = compile(&single).expect("compiles");
    assert_eq!(
        compiled.events[0].start, 47,
        "swung: its pair fits inside the pattern"
    );
    assert_eq!(
        compiled.events[1].start, 78,
        "not swung: its pair overruns the pattern"
    );

    let doubled = Song {
        arrangement: vec![0, 0],
        loop_start: 0,
        ..single
    };
    let compiled2 = compile(&doubled).expect("compiles");
    let second_slot_note_at_12 = compiled2
        .events
        .iter()
        .find(|e| e.slot == 1 && e.note == 0)
        .expect("second slot's first note");
    assert_eq!(
        second_slot_note_at_12.start, 141,
        "second slot uses pattern-relative phase but the run's own offset"
    );
}

// ---- 7. allocator (through the public `compile()`, `allocate()` itself is
// pub(crate) now: an unvalidated song must never reach it directly) --------

/// A one-row, one-pattern, one-slot song at 312.5 BPM (centi-tempo 31250):
/// with swing 0 that makes `step_ticks = 3750/312.5 = 12`, so a note's
/// compiled tick lands exactly on its `at`/`len` unit value (`floor(r/12*12
/// + 0.5) == r` for integer r). That lets these tests assert the exact
/// start/end numbers the allocator produces while going through the public
/// `compile()` API instead of calling `allocate()` directly.
fn tick_song(mask: u8, pattern_len: u32, notes: Vec<Note>) -> Song {
    Song {
        tempo: 31250,
        swing: 0,
        key: 0,
        voice_mask: mask,
        rows: vec![Row {
            sound: "kick".into(),
            note: None,
            vol: 100,
            pan: 0,
        }],
        patterns: vec![Pattern {
            name: "A".into(),
            length: pattern_len,
            tempo: None,
            notes,
        }],
        arrangement: vec![0],
        loop_start: 0,
    }
}

fn note_at(at: u32, len: u32, voice: Option<u32>) -> Note {
    Note {
        at,
        row: 0,
        len,
        vel: 100,
        voice,
        nudge: 0,
        end_nudge: 0,
    }
}

/// A pinned note takes its voice and cuts whatever was still sounding
/// there.
#[test]
fn allocate_pin_cuts_what_sounds_on_its_voice() {
    let song = tick_song(
        0b1,
        200,
        vec![note_at(0, 100, None), note_at(50, 150, Some(0))],
    );
    let compiled = compile(&song).expect("compiles");
    assert_eq!(compiled.events[0].voice, 0);
    assert_eq!(compiled.events[0].end, 50, "cut to the pinned note's start");
    assert_eq!(compiled.events[1].voice, 0);
    assert_eq!(compiled.events[1].end, 200);
}

/// An unpinned note takes the lowest voice in the mask that isn't busy.
#[test]
fn allocate_picks_lowest_free_voice() {
    let song = tick_song(
        0b0111,
        200,
        vec![
            note_at(0, 100, None),
            note_at(10, 100, None),
            note_at(20, 100, None),
        ],
    );
    let compiled = compile(&song).expect("compiles");
    assert_eq!(compiled.events[0].voice, 0);
    assert_eq!(compiled.events[1].voice, 1);
    assert_eq!(compiled.events[2].voice, 2);
}

/// With no free voice, the earliest-started voice is stolen (lowest voice
/// on a tie), and its event is cut to the stealing note's start.
#[test]
fn allocate_steals_earliest_started_voice_lowest_on_tie() {
    let song = tick_song(
        0b0011,
        1000,
        vec![
            note_at(0, 100, None),  // -> voice 0, started at 0
            note_at(0, 100, None),  // -> voice 1, started at 0 (tied with voice 0)
            note_at(50, 949, None), // no free voice; steals the tie's lowest voice
        ],
    );
    let compiled = compile(&song).expect("compiles");
    assert_eq!(compiled.events[0].voice, 0);
    assert_eq!(compiled.events[1].voice, 1);
    assert_eq!(compiled.events[2].voice, 0);
    assert_eq!(
        compiled.events[0].end, 50,
        "stolen event cut to the new note's start"
    );
    assert_eq!(
        compiled.events[1].end, 100,
        "the other tied voice is untouched"
    );
}

/// A voice outside the mask is never chosen, even under contention that
/// forces a steal.
#[test]
fn allocate_never_uses_a_voice_outside_the_mask() {
    let song = tick_song(
        0b1010_0000, // voices 5 and 7 only
        20,
        vec![
            note_at(0, 10, None),
            note_at(0, 10, None),
            note_at(0, 10, None),
        ],
    );
    let compiled = compile(&song).expect("compiles");
    for e in &compiled.events {
        assert!(
            e.voice == 5 || e.voice == 7,
            "voice {} outside the mask",
            e.voice
        );
    }
}

// ---- 8. stress ------------------------------------------------------------

/// A tiny deterministic LCG so the stress song is reproducible without a
/// dependency.
struct Lcg(u64);

impl Lcg {
    fn next_u32(&mut self) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 33) as u32
    }

    fn range(&mut self, n: u32) -> u32 {
        self.next_u32() % n
    }
}

/// A 10-minute, 200 BPM song: 16 one-bar (192-unit) patterns of ~42 notes
/// across 8 rows, played over a 500-slot arrangement (~21,000 notes).
fn build_stress_song() -> Song {
    let mut rng = Lcg(0x5EED_5EED_5EED_5EED);
    let rows: Vec<Row> = (0..8)
        .map(|i| Row {
            sound: format!("s{i}"),
            note: None,
            vol: 100,
            pan: 0,
        })
        .collect();
    let patterns: Vec<Pattern> = (0..16)
        .map(|_| {
            let notes = (0..42)
                .map(|_| Note {
                    at: rng.range(192),
                    row: rng.range(8),
                    len: 12 + rng.range(37), // 12..=48
                    vel: 1 + rng.range(127), // 1..=127
                    voice: None,
                    nudge: 0,
                    end_nudge: 0,
                })
                .collect();
            Pattern {
                name: "bar".into(),
                length: 192,
                tempo: None,
                notes,
            }
        })
        .collect();
    // 500 bars * 4 beats/bar / 200 BPM = 10 minutes.
    let arrangement: Vec<u32> = (0..500).map(|i| i % 16).collect();
    Song {
        tempo: 20000, // 200 BPM
        swing: 0,
        key: 0,
        voice_mask: 0xff,
        rows,
        patterns,
        arrangement,
        loop_start: 0,
    }
}

#[test]
fn stress_ten_minute_song_decodes_and_compiles_within_budget() {
    let song = build_stress_song();
    assert!(song.validate().is_ok());
    let bytes = encode(&song);

    let mut best = Duration::MAX;
    let mut last_events = 0usize;
    let mut last_length = 0i64;
    for _ in 0..5 {
        let start = Instant::now();
        let decoded = decode(&bytes).expect("decode");
        let compiled = compile(&decoded).expect("compile");
        let elapsed = start.elapsed();
        if elapsed < best {
            best = elapsed;
        }
        last_events = compiled.events.len();
        last_length = compiled.timing.length;
    }

    assert!(last_events >= 20_000, "events.len() = {last_events}");
    // 500 slots * 192 units, one continuous 200 BPM run: 96000/12 sixteenths
    // * (3750/200) ticks/16th = 150_000 ticks exactly.
    assert_eq!(last_length, 150_000);

    println!("decode+compile best of 5: {best:?}");
    if !cfg!(debug_assertions) {
        assert!(
            best < Duration::from_millis(10),
            "best {best:?} exceeded the 10ms budget"
        );
    }
}

// ---- 9. native `score{ song = }` playback, through the public LuaEngine
// API (`add_source`, `set_source`, `frame`, `audio`, `dsp_view`,
// `score_view`) — the codec/timing/allocator above are already proven in
// isolation; these exercise the NEW wiring: a `song`-kind source played by
// `score{}` with no Lua timer involved. ------------------------------------

/// `score{ song = "<name>" }` with no song source by that name is a setup
/// error naming it.
#[test]
fn score_with_no_such_song_source_names_it_in_the_error() {
    let err = LuaEngine::new()
        .set_source("score{ song = \"nope\" }")
        .unwrap_err();
    assert!(
        err.message.contains("no song source named 'nope'"),
        "got: {}",
        err.message
    );
}

/// `score{ data = ... }` is dropped entirely: it's a setup error pointing at
/// `song =` instead, not a silently accepted one-off table.
#[test]
fn score_data_is_dropped_and_errors_toward_song() {
    let err = LuaEngine::new()
        .set_source(
            "score{ data = { tempo = 120, rows = { { sound = \"kick\" } },\n\
               patterns = { A = { \"4.......\" } }, arrangement = { \"A\" } } }",
        )
        .unwrap_err();
    assert!(err.message.contains("song ="), "got: {}", err.message);
}

/// A small (~125-tick, ~0.5s) one-note looping song, used by the handle/
/// score_view tests below.
fn small_song(voice_mask: u8) -> Song {
    Song {
        tempo: 12000,
        swing: 0,
        key: 0,
        voice_mask,
        rows: vec![Row {
            sound: "kick".into(),
            note: None,
            vol: 100,
            pan: 0,
        }],
        patterns: vec![Pattern {
            name: "A".into(),
            length: 48,
            tempo: None,
            notes: vec![Note {
                at: 0,
                row: 0,
                len: 48,
                vel: 100,
                voice: None,
                nudge: 0,
                end_nudge: 0,
            }],
        }],
        arrangement: vec![0],
        loop_start: 0,
    }
}

/// The handle a song-sourced `score{}` returns reports its compiled length,
/// its tick advances, `loop` is true by default, `stop()` keys off and
/// holds the tick in place, and `play()` restarts it from the top.
#[test]
fn song_handle_reports_length_advances_and_stop_holds_the_tick() {
    let song = small_song(0xff);
    let want_length = compile(&song).expect("compiles").timing.length;

    let mut e = LuaEngine::new();
    common::add_song_source(&mut e, "beat", &song);
    e.set_source(
        "h = score{ song = \"beat\" }\n\
         function frame(t, f)\n\
           sram.length = h.length\n\
           sram.loop = h.loop\n\
           if f == 3 then sram.tick3 = h.tick end\n\
           if f == 4 then h.stop(); sram.at_stop = h.tick end\n\
           if f == 8 then sram.still = h.tick; h.play() end\n\
           if f == 9 then sram.restarted = h.tick end\n\
         end",
    )
    .unwrap();
    for f in 0..13u32 {
        e.frame(f as f64 / 60.0, f).unwrap();
    }
    let got: serde_json::Value = serde_json::from_str(&e.take_sram().unwrap()).unwrap();
    assert_eq!(got["length"], want_length);
    assert_eq!(got["loop"], true);
    assert!(got["tick3"].as_i64().unwrap() > 0, "tick advances: {got}");
    assert_eq!(got["still"], got["at_stop"], "stop() holds the tick: {got}");
    let restarted = got["restarted"].as_i64().unwrap();
    assert!(
        restarted > 0 && restarted < got["still"].as_i64().unwrap(),
        "play() restarts from the top and ticks: {got}"
    );
}

/// `loop = false` on a song source finishes and stops, same as the Lua path.
#[test]
fn song_handle_with_loop_false_finishes_and_stops_playing() {
    let song = small_song(0xff); // ~125 ticks == ~0.5s
    let mut e = LuaEngine::new();
    common::add_song_source(&mut e, "beat", &song);
    e.set_source(
        "h = score{ song = \"beat\", loop = false }\n\
         function frame() sram.playing = h.playing end",
    )
    .unwrap();
    // 120 frames at 60fps == 2s, 4x the song's length.
    for f in 0..120u32 {
        e.frame(f as f64 / 60.0, f).unwrap();
    }
    let got: serde_json::Value = serde_json::from_str(&e.take_sram().unwrap()).unwrap();
    assert_eq!(got["playing"], false, "a non-looping song must finish");
}

/// `score_view()` keeps a `loop = false` song's anchor once it finishes on
/// its own, flagged finished (an explicit `stop()` still goes absent).
#[test]
fn score_view_is_finished_once_a_loop_false_song_finishes() {
    let song = small_song(0xff); // ~125 ticks == ~0.5s
    let mut e = LuaEngine::new();
    common::add_song_source(&mut e, "beat", &song);
    e.set_source(
        "h = score{ song = \"beat\", loop = false }\n\
                  function frame() end",
    )
    .unwrap();
    // 120 frames at 60fps == 2s, 4x the song's length.
    for f in 0..120u32 {
        e.frame(f as f64 / 60.0, f).unwrap();
    }
    let v = e.score_view().expect("finished: still anchored");
    assert!(
        v.finished && v.anchor == Some(0.0) && v.tick == v.length,
        "{v:?}"
    );
}

/// `score_view()` (the studio playhead readout) reports a song source's
/// live tick/length/song name, and goes absent once it's stopped.
#[test]
fn score_view_reports_a_song_sources_tick_and_is_absent_when_stopped() {
    let song = small_song(0xff);
    let want_length = compile(&song).expect("compiles").timing.length;

    let mut e = LuaEngine::new();
    common::add_song_source(&mut e, "beat", &song);
    e.set_source(
        "h = score{ song = \"beat\" }\n\
         function frame(t, f) if f == 5 then h.stop() end end",
    )
    .unwrap();
    for f in 0..5u32 {
        e.frame(f as f64 / 60.0, f).unwrap();
        let v = e.score_view().expect("a playing song source reports");
        assert_eq!(v.song.as_deref(), Some("beat"));
        assert_eq!(v.length, want_length);
        assert!(v.tick < v.length);
    }
    e.frame(5.0 / 60.0, 5).unwrap(); // frame() calls h.stop() this frame
    assert_eq!(e.score_view(), None, "stopped: no readout");
}

/// A song with a 2-voice mask and three notes overlapping at the same start
/// only ever plays on voices 0-1 (the allocator's own contract, checked
/// directly on the compiled events), and the native playback hook never
/// touches a voice outside that mask. Observable: `frame()` stamps voice 5
/// with a fixed sentinel every frame; because an hdma/frame() voice[]
/// write only reaches the real DSP registers at the NEXT frame's offset-0
/// flush (see `flush_dsp_writes`'s doc comment), `dsp_view()` right after
/// `frame()` returns reflects whatever the LAST flush during THIS frame's
/// audio pass left there — the sentinel, unless the song hook itself wrote
/// voice 5 in between.
#[test]
fn native_song_never_touches_a_voice_outside_its_mask() {
    let song = Song {
        tempo: 12000,
        swing: 0,
        key: 0,
        voice_mask: 0b0000_0011,
        rows: vec![Row {
            sound: "kick".into(),
            note: None,
            vol: 100,
            pan: 0,
        }],
        patterns: vec![Pattern {
            name: "A".into(),
            length: 36,
            tempo: None,
            notes: vec![
                Note {
                    at: 0,
                    row: 0,
                    len: 36,
                    vel: 100,
                    voice: None,
                    nudge: 0,
                    end_nudge: 0,
                },
                Note {
                    at: 0,
                    row: 0,
                    len: 36,
                    vel: 100,
                    voice: None,
                    nudge: 0,
                    end_nudge: 0,
                },
                Note {
                    at: 0,
                    row: 0,
                    len: 36,
                    vel: 100,
                    voice: None,
                    nudge: 0,
                    end_nudge: 0,
                },
            ],
        }],
        arrangement: vec![0],
        loop_start: 0,
    };
    let compiled = compile(&song).expect("compiles");
    assert!(
        compiled.events.iter().all(|e| e.voice < 2),
        "the allocator must keep every event inside the song's mask: {:?}",
        compiled.events
    );

    let mut e = LuaEngine::new();
    common::add_song_source(&mut e, "beat", &song);
    e.set_source(
        "h = score{ song = \"beat\" }\n\
         function frame()\n\
           voice[5].sample = 42\n\
           voice[5].vol = { l = 77, r = 66 }\n\
         end",
    )
    .unwrap();

    for f in 0..30u32 {
        e.frame(f as f64 / 60.0, f).unwrap();
        if f >= 1 {
            let view = e.dsp_view();
            let v5 = &view.voices[5];
            assert_eq!(v5.sample, 42, "frame {f}: voice 5's sample was touched");
            assert_eq!(
                (v5.vol.l, v5.vol.r),
                (77, 66),
                "frame {f}: voice 5's vol was touched"
            );
        }
    }
}

/// A truncated `song`-kind source is a setup error at `score{}` time, and
/// the message names the chunk — the same shape `song::decode`'s own tests
/// exercise directly, proven here through the Lua boundary.
#[test]
fn a_truncated_song_source_names_the_chunk_at_score_setup() {
    let full = encode(&valid_song());
    let rows_pos = full
        .windows(4)
        .position(|w| w == b"ROWS")
        .expect("ROWS chunk present");
    let cut = rows_pos + 4 + 1 + 1; // tag + length byte + one body byte
    assert!(
        cut < full.len(),
        "ROWS chunk must have more than one body byte"
    );

    let mut e = LuaEngine::new();
    let mut payload = vec![3u8, 5];
    payload.extend_from_slice(&full[..cut]);
    e.add_source("beat", &payload).unwrap();
    let err = e.set_source("score{ song = \"beat\" }").unwrap_err();
    assert!(
        err.message.contains("ROWS chunk truncated"),
        "{}",
        err.message
    );
    assert!(
        err.message.contains("score: song 'beat':"),
        "{}",
        err.message
    );
}

/// An invalid song (a note's velocity over 127) is also a `score{}` setup
/// error, and the message names the pattern and the note.
#[test]
fn an_invalid_song_source_names_the_pattern_and_note_at_score_setup() {
    let mut song = valid_song();
    // 256, not e.g. 200: this note goes through encode()/decode() (via
    // common::add_song_source), and vel's wire byte shares its bit 7 with the
    // end-nudged flag. 200 has bit 7 set and would misdecode as end-nudged;
    // 256 doesn't, so it round-trips intact for validate() to still reject.
    song.patterns[0].notes[0].vel = 256;

    let mut e = LuaEngine::new();
    common::add_song_source(&mut e, "beat", &song);
    let err = e.set_source("score{ song = \"beat\" }").unwrap_err();
    assert!(
        err.message.contains("pattern 'A'")
            && err.message.contains("note 1")
            && err.message.contains("vel 256"),
        "{}",
        err.message
    );
}

/// `dma()` refuses a song source outright — a song plays through
/// `score{ song = }`, never through `dma()`'s VRAM/CGRAM/ARAM placement.
#[test]
fn dma_refuses_a_song_source() {
    let mut e = LuaEngine::new();
    common::add_song_source(&mut e, "beat", &valid_song());
    let err = e.set_source("dma(\"beat\")").unwrap_err();
    assert!(err.message.contains("beat"), "{}", err.message);
}

// ---- loop start -------------------------------------------------------

/// A 3-slot song (patterns of 48, 36 and 24 units at 120 BPM, no swing).
fn looped_song(loop_start: u32) -> Song {
    let mut song = valid_song();
    song.patterns = [48u32, 36, 24]
        .iter()
        .map(|&length| Pattern {
            name: format!("P{length}"),
            length,
            tempo: None,
            notes: vec![],
        })
        .collect();
    song.arrangement = vec![0, 1, 2];
    song.loop_start = loop_start;
    song
}

#[test]
fn loop_start_round_trips_as_version_2() {
    let mut song = sample_song();
    song.loop_start = 2;
    assert!(song.validate().is_ok());
    let bytes = encode(&song);
    assert_eq!(bytes[4], 2);
    assert_eq!(decode(&bytes).unwrap(), song);
    assert_eq!(encode(&decode(&bytes).unwrap()), bytes);
}

#[test]
fn loop_start_zero_encodes_as_unchanged_version_1() {
    let song = sample_song();
    let v1 = encode(&song);
    assert_eq!(v1[4], 1);
    // The ARRG chunk is last: tag, length, count, then one index per slot.
    let n = song.arrangement.len();
    let mut arrg = b"ARRG".to_vec();
    arrg.push((n + 1) as u8);
    arrg.push(n as u8);
    arrg.extend(song.arrangement.iter().map(|&p| p as u8));
    assert!(v1.ends_with(&arrg));
    // A nonzero loop start only adds the version bump and the trailing varint.
    // Layout, from the end: v1 ends `ARRG len count idx*n` (n + 6 bytes with
    // the 4-byte tag); v2 appends the loop varint, so its ARRG chunk starts
    // one byte earlier (n + 7). Everything before it (past the 5 byte header)
    // is identical.
    let mut looped = song.clone();
    looped.loop_start = 1;
    let v2 = encode(&looped);
    assert_eq!(v2.len(), v1.len() + 1);
    assert_eq!(&v2[5..v2.len() - n - 7], &v1[5..v1.len() - n - 6]);
    assert_eq!(v2[v2.len() - 1], 1);
}

#[test]
fn version_1_bytes_decode_with_loop_start_zero() {
    let song = sample_song();
    let mut bytes = encode(&song);
    // Hand-check the version byte, then confirm decode reads it as v1.
    assert_eq!(bytes[4], 1);
    assert_eq!(decode(&bytes).unwrap().loop_start, 0);
    // The same body under a v2 header lacks the trailing varint: truncated.
    bytes[4] = 2;
    assert!(decode(&bytes).is_err());
}

#[test]
fn version_2_with_trailing_bytes_is_rejected() {
    let mut song = sample_song();
    song.loop_start = 1;
    let mut bytes = encode(&song);
    // Append a stray byte to the ARRG body (the last chunk): bump its length.
    // From the end the v2 body is `count idx*n loop`, so the length byte
    // sits just before it: n + 2 body bytes after count, plus count itself.
    let n = song.arrangement.len();
    let len_at = bytes.len() - (n + 2) - 1;
    bytes[len_at] += 1;
    bytes.push(0);
    assert!(decode(&bytes).is_err());
}

#[test]
fn loop_start_json_omits_zero_and_defaults_absent() {
    let song = valid_song();
    let json = serde_json::to_value(&song).unwrap();
    assert!(json.get("loopStart").is_none());
    let back: Song = serde_json::from_value(json).unwrap();
    assert_eq!(back.loop_start, 0);
    let mut looped = valid_song();
    looped.arrangement = vec![0, 0];
    looped.loop_start = 1;
    let json = serde_json::to_value(&looped).unwrap();
    assert_eq!(json["loopStart"], 1);
}

#[test]
fn out_of_range_loop_start_is_rejected() {
    let mut song = looped_song(2);
    assert!(song.validate().is_ok());
    song.loop_start = 3;
    let err = song.validate().unwrap_err();
    assert!(
        err.0
            .contains("loop start slot 4 is past the arrangement (3 slots)"),
        "{}",
        err.0
    );
}

#[test]
fn wrap_pass_unwrap_over_a_looped_song() {
    let song = looped_song(1);
    let t = compile(&song).unwrap().timing;
    assert_eq!(t.loop_tick, t.tick(t.slots[1].pos));
    assert!(t.loop_tick > 0 && t.loop_tick < t.length);
    let body = t.length - t.loop_tick;

    // First pass: untouched, right up to the end.
    assert_eq!((t.wrap(0), t.pass(0)), (0, 0));
    assert_eq!(
        (t.wrap(t.length - 1), t.pass(t.length - 1)),
        (t.length - 1, 0)
    );
    // Exactly at the length: the loop point, pass 1.
    assert_eq!((t.wrap(t.length), t.pass(t.length)), (t.loop_tick, 1));
    assert_eq!(t.wrap(t.length + 5), t.loop_tick + 5);
    // The Nth pass.
    let n = t.length + 3 * body + 7;
    assert_eq!((t.wrap(n), t.pass(n)), (t.loop_tick + 7, 4));
    // unwrap inverts (pass, wrap).
    for n in 0..t.length + 5 * body {
        assert_eq!(t.unwrap(t.pass(n), t.wrap(n)), n, "n = {n}");
    }
    assert_eq!(t.unwrap(0, 10), 10);
    assert_eq!(t.unwrap(1, t.loop_tick), t.length);
}

#[test]
fn loop_start_zero_wraps_the_whole_song() {
    let t = compile(&looped_song(0)).unwrap().timing;
    assert_eq!(t.loop_tick, 0);
    assert_eq!(t.wrap(t.length), 0);
    assert_eq!((t.wrap(t.length * 2 + 3), t.pass(t.length * 2 + 3)), (3, 2));
}

/// With tempo-changing slots the loop tick comes off a later run's map.
#[test]
fn loop_tick_follows_the_run_map_across_tempo_changes() {
    let mut song = looped_song(2);
    song.patterns[1].tempo = Some(24000);
    song.patterns[2].tempo = Some(6000);
    let t = compile(&song).unwrap().timing;
    // Slot 2 is its own run: its start is its run_tick.
    assert_eq!(t.slots[2].run_pos, t.slots[2].pos);
    assert_eq!(t.loop_tick, t.slots[2].run_tick);
    assert_eq!(t.loop_tick, t.tick(t.slots[2].pos));
    let body = t.length - t.loop_tick;
    assert!(body > 0);
    for n in 0..t.length + 3 * body {
        assert_eq!(t.unwrap(t.pass(n), t.wrap(n)), n);
        assert!(t.wrap(n) < t.length);
    }
}

#[test]
fn analysis_reports_the_loop_tick() {
    let song = looped_song(2);
    let a = ppu_core::song_analyze::analyze(&song).unwrap();
    let t = compile(&song).unwrap().timing;
    assert_eq!(a.loop_tick, t.tick(t.slots[2].pos));
    assert_eq!(a.loop_tick, t.loop_tick);
    assert_eq!(
        ppu_core::song_analyze::analyze(&looped_song(0))
            .unwrap()
            .loop_tick,
        0
    );
}
