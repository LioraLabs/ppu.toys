//! `ppu_core::song`: the PSNG codec, timing map, compiler and voice
//! allocator, exercised entirely through the public API.

use ppu_core::song::*;
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
                    },
                    Note {
                        at: 0,
                        row: 1,
                        len: 6,
                        vel: 64,
                        voice: Some(3),
                        nudge: -5,
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
                    },
                    Note {
                        at: 50,
                        row: 1,
                        len: 1,
                        vel: 1,
                        voice: None,
                        nudge: 0,
                    },
                ],
            },
        ],
        arrangement: vec![0, 1, 0],
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
            }],
        }],
        arrangement: vec![0],
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

/// A version other than 1 is rejected by number.
#[test]
fn unsupported_version_is_rejected() {
    let mut bytes = encode(&sample_song());
    bytes[4] = 2;
    let err = decode(&bytes).unwrap_err();
    assert_eq!(err.0, "unsupported PSNG version 2");
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
            },
            // len 10000 always clamps to the song's end -> forces overlap.
            Note {
                at: 24,
                row: 0,
                len: 10000,
                vel: 127,
                voice: None,
                nudge: 0,
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
            },
            Note {
                at: 12,
                row: 0,
                len: 12,
                vel: 90,
                voice: None,
                nudge: 3,
            },
            Note {
                at: 24,
                row: 0,
                len: 12,
                vel: 50,
                voice: None,
                nudge: -2,
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
        row: 0,
        voice,
        pin,
        l,
        r,
        slot,
        note,
    };
    let expected = vec![
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
    assert_eq!(compiled.events, expected);
}

// ---- 7. allocator --------------------------------------------------------

fn ev_at(start: i64, end: i64, pin: Option<u8>) -> Event {
    Event {
        start,
        end,
        row: 0,
        voice: 0,
        pin,
        l: 0,
        r: 0,
        slot: 0,
        note: 0,
    }
}

/// A pinned note takes its voice and cuts whatever was still sounding
/// there.
#[test]
fn allocate_pin_cuts_what_sounds_on_its_voice() {
    let mut events = vec![ev_at(0, 100, None), ev_at(50, 200, Some(0))];
    allocate(&mut events, 0b1);
    assert_eq!(events[0].voice, 0);
    assert_eq!(events[0].end, 50, "cut to the pinned note's start");
    assert_eq!(events[1].voice, 0);
    assert_eq!(events[1].end, 200);
}

/// An unpinned note takes the lowest voice in the mask that isn't busy.
#[test]
fn allocate_picks_lowest_free_voice() {
    let mut events = vec![
        ev_at(0, 100, None),
        ev_at(10, 100, None),
        ev_at(20, 100, None),
    ];
    allocate(&mut events, 0b0111);
    assert_eq!(events[0].voice, 0);
    assert_eq!(events[1].voice, 1);
    assert_eq!(events[2].voice, 2);
}

/// With no free voice, the earliest-started voice is stolen (lowest voice
/// on a tie), and its event is cut to the stealing note's start.
#[test]
fn allocate_steals_earliest_started_voice_lowest_on_tie() {
    let mut events = vec![
        ev_at(0, 100, None),  // -> voice 0, started at 0
        ev_at(0, 100, None),  // -> voice 1, started at 0 (tied with voice 0)
        ev_at(50, 999, None), // no free voice; steals the tie's lowest voice
    ];
    allocate(&mut events, 0b0011);
    assert_eq!(events[0].voice, 0);
    assert_eq!(events[1].voice, 1);
    assert_eq!(events[2].voice, 0);
    assert_eq!(
        events[0].end, 50,
        "stolen event cut to the new note's start"
    );
    assert_eq!(events[1].end, 100, "the other tied voice is untouched");
}

/// A voice outside the mask is never chosen, even under contention that
/// forces a steal.
#[test]
fn allocate_never_uses_a_voice_outside_the_mask() {
    let mut events = vec![ev_at(0, 10, None), ev_at(0, 10, None), ev_at(0, 10, None)];
    allocate(&mut events, 0b1010_0000); // voices 5 and 7 only
    for e in &events {
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
