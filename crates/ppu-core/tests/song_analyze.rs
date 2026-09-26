//! `ppu_core::song_analyze`: per-step voice counts, through the public API.

use ppu_core::song::*;
use ppu_core::song_analyze::analyze;

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

/// The sweep agrees with a steps x events brute force on a busy song with
/// swing, mixed tempos, odd lengths, pins and nudges.
#[test]
fn sweep_matches_brute_force() {
    let mut seed = 7u32;
    let mut rnd = |n: u32| {
        seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
        (seed >> 8) % n
    };
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
                    .map(|_| Note {
                        at: rnd(length),
                        row: 0,
                        len: 1 + rnd(60),
                        vel: 100,
                        voice: if rnd(5) == 0 { Some(rnd(3) * 2) } else { None },
                        nudge: rnd(7) as i32,
                    })
                    .collect(),
            }
        })
        .collect();
    let mut s = song(0b0101_0101, patterns, vec![0, 1, 1, 2, 3, 0, 2]);
    s.swing = 40;
    let a = analyze(&s).unwrap();
    let c = compile(&s).unwrap();
    assert_eq!(a.events, c.events);

    let t = &c.timing;
    let mut ticks = Vec::new();
    for sl in &t.slots {
        let mut u = 0u64;
        while u < sl.len as u64 {
            ticks.push(t.tick(sl.pos + u));
            u += 12;
        }
    }
    let uncut: Vec<i64> = c
        .events
        .iter()
        .map(|e| {
            let sl = &t.slots[e.slot as usize];
            let n = &s.patterns[sl.pattern as usize].notes[e.note as usize];
            t.tick((sl.pos + (n.at + n.len) as u64).min(t.end))
                .max(e.start)
        })
        .collect();
    let used: Vec<u8> = ticks
        .iter()
        .map(|&x| {
            c.events
                .iter()
                .filter(|e| e.start <= x && x < e.end)
                .count() as u8
        })
        .collect();
    let wanted: Vec<u32> = ticks
        .iter()
        .map(|&x| {
            c.events
                .iter()
                .zip(&uncut)
                .filter(|(e, &end)| e.start <= x && x < end)
                .count() as u32
        })
        .collect();
    assert!(
        wanted.iter().any(|&w| w > 4),
        "the song should overflow somewhere"
    );
    assert_eq!(a.used, used);
    assert_eq!(a.wanted, wanted);
    let over: Vec<u32> = (0..wanted.len() as u32)
        .filter(|&i| wanted[i as usize] > 4)
        .collect();
    assert_eq!(a.over, over);
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
