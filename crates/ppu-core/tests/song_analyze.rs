//! `ppu_core::song_analyze`: per-step voice counts, through the public API.

use ppu_core::song::*;
use ppu_core::song_analyze::{analyze, Analysis};

fn note(at: u32, len: u32, voice: Option<u32>) -> Note {
    Note {
        at,
        row: 0,
        len,
        vel: 100,
        voice,
        nudge: 0,
    }
}

fn song(voice_mask: u8, patterns: Vec<Pattern>, arrangement: Vec<u32>) -> Song {
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
        patterns,
        arrangement,
    }
}

/// 120 BPM, two voices: a 16th is 31.25 ticks, so steps sit at ticks 0, 31,
/// 63, 94 in A and 125, 156 in the 18-unit B (two steps: ceil(18/12)).
/// Events: n1 [0,125) v0 is stolen at 31 by n3 [31,63); n2 [31,94) v1 is cut
/// at 63 by n4, pinned to v1 [63,125); n5 [125,172) takes the free v0.
#[test]
fn counts_used_wanted_and_over_by_hand() {
    let s = song(
        0b11,
        vec![
            Pattern {
                name: "A".into(),
                length: 48,
                tempo: None,
                notes: vec![
                    note(0, 48, None),
                    note(12, 24, None),
                    note(12, 12, None),
                    note(24, 24, Some(1)),
                ],
            },
            Pattern {
                name: "B".into(),
                length: 18,
                tempo: None,
                notes: vec![note(0, 18, None)],
            },
        ],
        vec![0, 1],
    );
    let a = analyze(&s).unwrap();
    let ev: Vec<(i64, i64, u8)> = a.events.iter().map(|e| (e.start, e.end, e.voice)).collect();
    assert_eq!(
        ev,
        vec![
            (0, 31, 0),
            (31, 63, 1),
            (31, 63, 0),
            (63, 125, 1),
            (125, 172, 0)
        ]
    );
    assert_eq!(a.length, 172);
    assert_eq!(a.slot_steps, vec![0, 4]);
    assert_eq!(a.used, vec![1, 2, 1, 1, 1, 1]);
    assert_eq!(a.wanted, vec![1, 3, 3, 2, 1, 1]);
    assert_eq!(a.over, vec![1, 2]);
}

/// One pattern of `length` units, arranged once.
fn one(voice_mask: u8, length: u32, notes: Vec<Note>) -> Song {
    song(
        voice_mask,
        vec![Pattern {
            name: "A".into(),
            length,
            tempo: None,
            notes,
        }],
        vec![0],
    )
}

/// (used, wanted, over) as vectors.
fn counts(s: &Song) -> (Vec<u8>, Vec<u32>, Vec<u32>) {
    let a = analyze(s).unwrap();
    assert_eq!(a, brute(s), "sweep disagrees with the tick brute force");
    (a.used, a.wanted, a.over)
}

/// `n` copies of `note` (a chord on one start).
fn chord(n: usize, note: Note) -> Vec<Note> {
    vec![note; n]
}

/// Three stacked 32nds inside one 16th on one voice: two are cut to nothing.
#[test]
fn stacked_32nds_between_steps_count() {
    let s = one(0b1, 24, chord(3, note(6, 6, None)));
    assert_eq!(counts(&s), (vec![1, 0], vec![3, 0], vec![0]));
    let s = one(0xff, 24, chord(9, note(3, 6, None)));
    assert_eq!(counts(&s), (vec![8, 0], vec![9, 0], vec![0]));
}

/// 32nds played one after another are one voice, not two per step.
#[test]
fn consecutive_32nds_are_not_double_counted() {
    let s = one(0b1, 24, (0..4).map(|i| note(i * 6, 6, None)).collect());
    assert_eq!(counts(&s), (vec![1, 1], vec![1, 1], vec![]));
}

/// 16th triplets (8 units) and 8th triplets (16 units).
#[test]
fn triplets_count() {
    // Two-note 16th-triplet chords on one voice: every step wants 2.
    let notes = (0..3)
        .flat_map(|i| chord(2, note(i * 8, 8, None)))
        .collect();
    let s = one(0b1, 24, notes);
    assert_eq!(counts(&s), (vec![1, 1], vec![2, 2], vec![0, 1]));

    // 8th triplets on 8 voices; nine notes on the second triplet (unit 16,
    // mid-step 1) spill past the mask in steps 1 and 2 only.
    let mut notes = vec![note(0, 16, None), note(32, 16, None)];
    notes.extend(chord(9, note(16, 16, None)));
    let s = one(0xff, 48, notes);
    assert_eq!(counts(&s), (vec![1, 8, 8, 1], vec![1, 9, 9, 1], vec![1, 2]));
}

/// A note nudged early into the previous step counts there.
#[test]
fn nudged_notes_count_in_the_step_they_start() {
    // At 120 BPM step 1 starts at tick 31; nudge -10 starts it at 21.
    let mut late = note(12, 12, None);
    late.nudge = -10;
    let s = one(0b1, 24, vec![note(0, 12, None), late.clone()]);
    assert_eq!(counts(&s), (vec![1, 1], vec![2, 1], vec![0]));

    let mut notes = chord(8, note(0, 12, None));
    notes.push(late);
    let s = one(0xff, 24, notes);
    assert_eq!(counts(&s), (vec![8, 1], vec![9, 1], vec![0]));
}

/// The sweep agrees with a tick-by-tick brute force on busy songs with
/// swing, mixed tempos, odd lengths, off-grid starts, pins and nudges, in
/// 1-, 4- and 8-voice masks.
#[test]
fn sweep_matches_brute_force() {
    let mut seed = 7u32;
    let mut rnd = |n: u32| {
        seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
        (seed >> 8) % n
    };
    for mask in [0b1u8, 0b0101_0101, 0xff] {
        let pins: Vec<u32> = (0..8).filter(|v| mask & (1 << v) != 0).collect();
        let patterns: Vec<Pattern> = (0..4)
            .map(|p| {
                let length = 30 + rnd(90);
                Pattern {
                    name: format!("p{p}"),
                    length,
                    tempo: if p % 2 == 1 {
                        Some(9000 + rnd(9000))
                    } else {
                        None
                    },
                    notes: (0..40)
                        .map(|_| {
                            let at = rnd(length);
                            Note {
                                at,
                                row: 0,
                                len: 1 + rnd(30),
                                vel: 100,
                                voice: if rnd(5) == 0 {
                                    Some(pins[rnd(pins.len() as u32) as usize])
                                } else {
                                    None
                                },
                                // Never early at unit 0: that would start before the song.
                                nudge: rnd(15) as i32 - if at == 0 { 0 } else { 3 },
                            }
                        })
                        .collect(),
                }
            })
            .collect();
        let mut s = song(mask, patterns, vec![0, 1, 1, 2, 3, 0, 2]);
        s.swing = 40;
        let a = analyze(&s).unwrap();
        assert_eq!(a.events, compile(&s).unwrap().events);
        assert!(
            !a.over.is_empty(),
            "mask {mask:#b} should overflow somewhere"
        );
        assert_eq!(a, brute(&s), "mask {mask:#b}");
    }
}

/// At 400 BPM a 13-unit pattern's short last step can share its tick with
/// the next slot's first: that step spans no ticks and holds nothing.
#[test]
fn a_step_with_no_ticks_counts_nothing() {
    let mut s = one(0b1, 13, vec![note(0, 13, None)]);
    s.tempo = 40000;
    s.arrangement = vec![0, 0, 0];
    let a = analyze(&s).unwrap();
    assert_eq!(a, brute(&s));
    let ticks: Vec<i64> = a.events.iter().map(|e| e.start).collect();
    assert_eq!(ticks, vec![0, 10, 20]);
    assert_eq!(a.used, vec![1, 1, 1, 0, 1, 0]); // steps at ticks 0, 9, 10, 20, 20, 30
}

/// `analyze` by definition: for each step, every tick from its own up to
/// the next step's (the last runs to the latest note end), counting the
/// events, and the uncut notes, sounding there.
fn brute(s: &Song) -> Analysis {
    let c = compile(s).unwrap();
    let t = &c.timing;
    let mut ticks = Vec::new();
    let mut slot_steps = Vec::new();
    for sl in &t.slots {
        slot_steps.push(ticks.len() as u32);
        let mut u = 0u64;
        while u < sl.len as u64 {
            ticks.push(t.tick(sl.pos + u));
            u += 12;
        }
    }
    let last = c.events.iter().map(|e| e.want_end).max().unwrap_or(0);
    let peak = |i: usize, end: &dyn Fn(&Event) -> i64| {
        let to = ticks.get(i + 1).copied().unwrap_or(last.max(ticks[i] + 1));
        (ticks[i]..to)
            .map(|x| {
                c.events
                    .iter()
                    .filter(|e| e.start <= x && x < end(e))
                    .count()
            })
            .max()
            .unwrap_or(0)
    };
    let used: Vec<u8> = (0..ticks.len())
        .map(|i| peak(i, &|e| e.end) as u8)
        .collect();
    let wanted: Vec<u32> = (0..ticks.len())
        .map(|i| peak(i, &|e| e.want_end) as u32)
        .collect();
    let voices = s.voice_mask.count_ones();
    let over = (0..wanted.len() as u32)
        .filter(|&i| wanted[i as usize] > voices)
        .collect();
    Analysis {
        length: t.length,
        events: c.events,
        slot_steps,
        used,
        wanted,
        over,
    }
}

#[test]
fn errors_match_compile_and_cap_huge_songs() {
    let mut bad = song(
        0b1,
        vec![Pattern {
            name: "A".into(),
            length: 48,
            tempo: None,
            notes: vec![note(0, 12, None)],
        }],
        vec![0],
    );
    bad.patterns[0].notes[0].vel = 200;
    assert_eq!(analyze(&bad).unwrap_err(), compile(&bad).unwrap_err());

    let huge = song(
        0b1,
        vec![Pattern {
            name: "A".into(),
            length: 4_000_000_000,
            tempo: None,
            notes: vec![],
        }],
        vec![0],
    );
    assert!(analyze(&huge).unwrap_err().0.contains("sixteenths long"));
}
