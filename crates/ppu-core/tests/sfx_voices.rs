//! sfx() with no voice plays on the voice `ppu_rules::voices::choose` picks, and every effect,
//! an explicit voice included, borrows its voice from the song until it ends: the song's notes
//! there are silent meanwhile, and its next note after the effect plays.

mod common;

use ppu_core::song::{Note, Pattern, Row, Song};
use ppu_core::LuaEngine;

/// `chord` simultaneous organ notes on every quarter (0.5 s, 30 frames), each held 25 frames:
/// rows play MIDI 60.. so each voice's pitch names its row.
fn chords(chord: u32) -> Song {
    Song {
        tempo: 12000,
        swing: 0,
        key: 0,
        voice_mask: 0xff,
        rows: (0..chord)
            .map(|i| Row {
                sound: "organ".into(),
                note: Some(60 + i),
                vol: 100,
                pan: 0,
                keep: false,
            })
            .collect(),
        patterns: vec![Pattern {
            name: "A".into(),
            length: 16 * 12,
            tempo: None,
            notes: (0..4)
                .flat_map(|q| {
                    (0..chord).map(move |row| Note {
                        at: q * 48,
                        row,
                        len: 40,
                        vel: 100,
                        voice: None,
                        nudge: 0,
                        end_nudge: 0,
                    })
                })
                .collect(),
        }],
        arrangement: vec![0],
        loop_start: 0,
    }
}

/// An engine playing `chords(chord)` (when `chord > 0`) whose frame body is `body`, run to `last`;
/// returns, per frame, each voice's (pitch, envx).
fn run(chord: u32, body: &str, last: u32) -> Vec<[(u16, u8); 8]> {
    run_song(chord, chords(chord), body, last)
}

fn run_song(chord: u32, song: Song, body: &str, last: u32) -> Vec<[(u16, u8); 8]> {
    let mut e = LuaEngine::new();
    if chord > 0 {
        common::add_song_source(&mut e, "chords", &song);
    }
    let score = if chord > 0 {
        "score{ song = \"chords\" }"
    } else {
        ""
    };
    let main = format!(
        "dsp.mvol = {{l = 127, r = 127}}\nlocal organ = bank('organ')\n{score}\n\
         function frame(t, f)\n{body}\nend"
    );
    e.set_sources(&[("main.lua", &main)]).unwrap();
    (0..=last)
        .map(|f| {
            e.frame(f as f64 / 60.0, f).unwrap();
            let d = e.dsp_view();
            std::array::from_fn(|v| (d.voices[v].pitch, d.voices[v].envx))
        })
        .collect()
}

/// The pitch `note("C2")` gives an organ recorded at C4.
const C2: u16 = 0x400;

#[test]
fn with_no_song_an_automatic_effect_takes_the_highest_voice() {
    let seen = run(0, "if f == 0 then assert(sfx(organ, 'C2') == 7) end", 4);
    assert_eq!(seen[3][7].0, C2);
    assert!(seen[3][7].1 > 0, "it sounds: {:?}", seen[3]);
}

#[test]
fn the_automatic_form_takes_options_and_the_explicit_voice_still_works() {
    let seen = run(
        0,
        "if f == 0 then sfx(organ, 'C2', {len = 0.4}) sfx(organ, 5, 'C2') end",
        30,
    );
    assert!(seen[20][7].1 > 0, "0.4 s is 24 frames: {:?}", seen[20]);
    assert_eq!(seen[30][7].1, 0, "and then it stops: {:?}", seen[30]);
    assert_eq!(seen[3][5].0, C2, "voice 5: {:?}", seen[3]);
    assert!(seen[3][5].1 > 0);
}

#[test]
fn a_song_on_four_voices_leaves_the_effect_a_free_one() {
    let seen = run(
        4,
        "if f == 40 then assert(sfx(organ, 'C2', {len = 0.4}) == 7) end",
        70,
    );
    let song = seen[10];
    assert!(
        (0..4).all(|v| song[v].1 > 0) && (4..8).all(|v| song[v].1 == 0),
        "{song:?}"
    );
    assert_eq!(seen[62][7].0, C2);
    // The song plays on untouched.
    assert_eq!(
        (0..4).map(|v| seen[62][v].0).collect::<Vec<_>>(),
        (0..4).map(|v| song[v].0).collect::<Vec<_>>()
    );
}

#[test]
fn an_automatic_effect_over_a_full_chord_borrows_the_highest_voice_and_hands_it_back() {
    // At frame 40 every voice rests 20 frames before the next chord: shorter than the effect's
    // 24, so it steals voice 7, and the frame-60 chord leaves that voice to the effect.
    let seen = run(
        8,
        "if f == 40 then assert(sfx(organ, 'C2', {len = 0.4}) == 7) end",
        100,
    );
    let song = seen[10][7].0;
    assert_ne!(song, C2);
    assert_eq!(
        seen[62][7].0, C2,
        "the song's key-on does not cut the effect"
    );
    assert!(seen[62][7].1 > 0, "{:?}", seen[62]);
    assert_eq!(
        seen[62][6].0, seen[10][6].0,
        "voice 6 plays the song's chord"
    );
    assert_eq!(
        seen[92][7].0, song,
        "after the effect, the song's next note on 7 plays"
    );
    assert!(seen[92][7].1 > 0);
}

#[test]
fn a_short_effect_fits_a_rest() {
    // 0.15 s (38 ticks) fits the 20-frame rest: voice 7, as no voice is free, but the song's
    // next chord plays on it.
    let seen = run(8, "if f == 40 then assert(sfx(organ, 'C2') == 7) end", 70);
    assert_eq!(seen[45][7].0, C2);
    assert_eq!(
        seen[62][7].0, seen[10][7].0,
        "the effect ended before the chord"
    );
}

#[test]
fn an_explicit_voice_borrows_too() {
    let seen = run(
        8,
        "if f == 40 then sfx(organ, 5, 'C2', {len = 0.4}) end",
        100,
    );
    assert_eq!(
        seen[62][5].0, C2,
        "the song's key-on does not cut the effect"
    );
    assert!(seen[62][5].1 > 0);
    assert_eq!(
        seen[92][5].0, seen[10][5].0,
        "the song's next note on 5 plays"
    );
}

#[test]
fn effects_fill_the_free_voices_then_cut_the_oldest_automatic_one() {
    let seen = run(
        0,
        "if f == 0 then\n\
           for want = 7, 1, -1 do assert(sfx(organ, 'C2', {len = 1}) == want) end\n\
           sfx(organ, 0, 'C2', {len = 1})\n\
           assert(sfx(organ, 'C3', {len = 1}) == 7, 'the oldest automatic effect')\n\
           assert(sfx(organ, 'C3', {len = 1}) == 6)\n\
         end",
        3,
    );
    assert_eq!(seen[2][7].0, 0x800);
    assert_eq!(seen[2][0].0, C2);
}

#[test]
fn a_burst_of_effects_never_takes_a_kept_part() {
    // Rows 0 and 1 (the lead) are kept: the allocator gives them voices 0 and 1, and eight
    // automatic effects over the full chord take only the six others, the last two cutting the
    // oldest effects.
    let mut song = chords(8);
    song.rows[0].keep = true;
    song.rows[1].keep = true;
    let seen = run_song(
        8,
        song,
        "if f == 40 then\n\
           for _ = 1, 8 do local v = sfx(organ, 'C2', {len = 1}) assert(v ~= nil and v >= 2, v) end\n\
         end",
        100,
    );
    let lead = [seen[10][0].0, seen[10][1].0];
    assert!(!lead.contains(&C2) && lead[0] != lead[1], "{lead:?}");
    for (f, voices) in seen.iter().enumerate().skip(10) {
        assert_eq!([voices[0].0, voices[1].0], lead, "frame {f}: {voices:?}");
    }
    for f in [62, 92] {
        assert!(seen[f][0].1 > 0 && seen[f][1].1 > 0, "the lead's note at {f}: {:?}", seen[f]);
        assert!((2..8).all(|v| seen[f][v].0 == C2), "the effects at {f}: {:?}", seen[f]);
    }
}
