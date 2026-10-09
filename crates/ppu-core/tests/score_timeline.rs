//! A native `score{}` is anchored to the timeline: its position is
//! (t - anchor) * 250 ticks, so jumping `f` seeks the song, `at` places the
//! anchor, `play()` re-anchors at the current t, and a recompile resumes at
//! t. Driven through `set_sources`/`frame`/`score_view`/`take_sram`/`aram`.

mod common;

use ppu_core::song::{Note, Pattern, Row, Song};
use ppu_core::LuaEngine;
use serde_json::Value;

/// 120 BPM, 16 steps of a 16th each (31.25 ticks, 2 s in all); step `i` is
/// its own row with its own note, so the pitch a voice is keyed with names
/// the step that played.
fn steps() -> Song {
    Song {
        tempo: 12000,
        swing: 0,
        key: 0,
        voice_mask: 0xff,
        rows: (0..16)
            .map(|i| Row {
                sound: "kick".into(),
                note: Some(48 + i),
                vol: 100,
                pan: 0,
            })
            .collect(),
        patterns: vec![Pattern {
            name: "A".into(),
            length: 16 * 12,
            tempo: None,
            notes: (0..16)
                .map(|i| Note {
                    at: i * 12,
                    row: i,
                    len: 12,
                    vel: 100,
                    voice: None,
                    nudge: 0,
                    end_nudge: 0,
                })
                .collect(),
        }],
        arrangement: vec![0],
    }
}

/// Logs every key-on a frame body sees (the native player writes a fresh
/// `vol` table per key-on) as `{ f, p }`: the frame that saw it — events
/// played during frame f's audio show up in frame f + 1 — and the pitch.
const LOG: &str = "heard, last = {}, {}\n\
     for v = 0, 7 do last[v] = voice[v].vol end\n\
     function frame(t, f)\n\
       for v = 0, 7 do\n\
         if voice[v].vol ~= last[v] then\n\
           heard[#heard + 1] = { f = f, p = voice[v].pitch }\n\
           last[v] = voice[v].vol\n\
         end\n\
       end\n\
       if body then body(f) end\n\
       sram.heard = heard sram.playing = h.playing\n\
     end";

fn engine(score: &str) -> LuaEngine {
    let mut e = LuaEngine::new();
    common::add_song_source(&mut e, "steps", &steps());
    e.set_sources(&[("main.lua", &program(score))]).unwrap();
    e
}

fn program(score: &str) -> String {
    format!("{LOG}\n{score}")
}

fn run(e: &mut LuaEngine, frames: std::ops::Range<u32>) -> Value {
    for f in frames {
        e.frame(f as f64 / 60.0, f).unwrap();
    }
    serde_json::from_str(&e.take_sram().expect("frame wrote sram")).unwrap()
}

/// `(frame that saw it, step)` for every key-on logged after the first
/// `skip` and seen in a frame after `after`.
fn heard(sram: &Value, skip: usize, after: u32) -> Vec<(u32, usize)> {
    let pitches = step_pitches();
    sram["heard"].as_array().unwrap()[skip..]
        .iter()
        .map(|k| (k["f"].as_u64().unwrap() as u32, k["p"].as_i64().unwrap()))
        .filter(|(f, _)| *f > after)
        .map(|(f, p)| (f, pitches.iter().position(|&q| q == p).expect("a step")))
        .collect()
}

fn log_len(sram: &Value) -> usize {
    sram["heard"].as_array().unwrap().len()
}

/// Each step's pitch, from one continuous pass.
fn step_pitches() -> Vec<i64> {
    let mut e = LuaEngine::new();
    common::add_song_source(&mut e, "steps", &steps());
    e.set_sources(&[("main.lua", &program("h = score{ song = \"steps\" }"))])
        .unwrap();
    let sram = run(&mut e, 0..121);
    let mut out: Vec<i64> = Vec::new();
    for k in sram["heard"].as_array().unwrap() {
        let p = k["p"].as_i64().unwrap();
        if !out.contains(&p) {
            out.push(p);
        }
    }
    assert_eq!(out.len(), 16, "one pitch per step: {out:?}");
    out
}

#[test]
fn a_forward_seek_resumes_at_the_next_note_due() {
    let mut e = engine("h = score{ song = \"steps\" }");
    run(&mut e, 0..10);
    // Frame 90 starts at t = 1.5 s: tick 374, and step 12 starts at 375.
    let sram = run(&mut e, 90..100);
    assert_eq!(heard(&sram, 0, 90).first(), Some(&(91, 12)), "{sram}");
}

#[test]
fn a_backward_seek_resumes_at_the_next_note_due() {
    let mut e = engine("h = score{ song = \"steps\" }");
    let seen = log_len(&run(&mut e, 0..100));
    // Frame 30: t = 0.5 s, tick 124; step 4 starts at 125.
    let sram = run(&mut e, 30..40);
    assert_eq!(heard(&sram, seen, 30).first(), Some(&(31, 4)), "{sram}");
}

#[test]
fn before_its_anchor_a_song_is_silent_then_starts_from_the_top() {
    let mut e = engine("h = score{ song = \"steps\", at = 1 }");
    let sram = run(&mut e, 0..70);
    assert_eq!(heard(&sram, 0, 0).first(), Some(&(61, 0)), "{sram}");

    // Seek back before the anchor: silent until it is reached again.
    let seen = log_len(&sram);
    let sram = run(&mut e, 30..70);
    assert_eq!(heard(&sram, seen, 30).first(), Some(&(61, 0)), "{sram}");
}

#[test]
fn a_seek_past_the_end_of_a_loop_false_song_is_silent() {
    let mut e = engine("h = score{ song = \"steps\", loop = false }");
    run(&mut e, 0..10);
    let sram = run(&mut e, 150..170);
    assert_eq!(heard(&sram, 0, 150), vec![], "{sram}");
    assert_eq!(sram["playing"], false);
    assert_eq!(e.score_view(), None);
}

#[test]
fn a_seek_into_a_later_pass_of_a_looping_song_wraps() {
    let mut e = engine("h = score{ song = \"steps\" }");
    run(&mut e, 0..10);
    // Frame 210: t = 3.5 s, tick 874 = 500 + 374.
    let sram = run(&mut e, 210..220);
    assert_eq!(heard(&sram, 0, 210).first(), Some(&(211, 12)), "{sram}");
}

#[test]
fn at_names_a_marker() {
    let mut e = engine("markers = { intro = 0.75 }\nh = score{ song = \"steps\", at = \"intro\" }");
    let sram = run(&mut e, 0..50);
    assert_eq!(e.score_view().unwrap().anchor, Some(0.75));
    // 0.75 s is frame 45's start.
    assert_eq!(heard(&sram, 0, 0).first(), Some(&(46, 0)), "{sram}");
}

#[test]
fn an_unknown_marker_is_a_setup_error_naming_it() {
    let mut e = LuaEngine::new();
    common::add_song_source(&mut e, "steps", &steps());
    let err = e
        .set_sources(&[(
            "main.lua",
            &program("markers = { intro = 1 }\nh = score{ song = \"steps\", at = \"outro\" }"),
        )])
        .unwrap_err();
    assert!(
        err.message.contains("no marker named 'outro'"),
        "{}",
        err.message
    );
}

#[test]
fn at_false_waits_for_play_which_anchors_at_that_frame() {
    let mut e = engine(
        "h = score{ song = \"steps\", at = false }\n\
         function body(f) if f == 30 then h.play() end end",
    );
    run(&mut e, 0..30);
    assert_eq!(e.score_view(), None, "set up, not playing");
    let sram = run(&mut e, 30..40);
    assert_eq!(e.score_view().unwrap().anchor, Some(0.5));
    assert_eq!(heard(&sram, 0, 0).first(), Some(&(31, 0)), "{sram}");
}

#[test]
fn stop_then_play_restarts_from_the_top() {
    let mut e = engine(
        "h = score{ song = \"steps\" }\n\
         function body(f) if f == 40 then h.stop() end if f == 50 then h.play() end end",
    );
    let sram = run(&mut e, 0..60);
    let after: Vec<_> = heard(&sram, 0, 40);
    assert_eq!(
        after.first(),
        Some(&(51, 0)),
        "silent while stopped: {sram}"
    );
    // Frame 50 starts at sample 26667.
    assert_eq!(e.score_view().unwrap().anchor, Some(26667.0 / 32000.0));
}

#[test]
fn the_default_anchor_is_zero() {
    let mut e = engine("h = score{ song = \"steps\" }");
    run(&mut e, 0..2);
    assert_eq!(e.score_view().unwrap().anchor, Some(0.0));
}

#[test]
fn a_recompile_resumes_the_song_at_t() {
    let mut e = engine("h = score{ song = \"steps\" }");
    run(&mut e, 0..90);
    let edited = program("h = score{ song = \"steps\" } -- edited");
    e.set_sources(&[("main.lua", &edited)]).unwrap();
    let sram = run(&mut e, 90..100);
    assert_eq!(heard(&sram, 0, 0).first(), Some(&(91, 12)), "{sram}");
}

/// The echo buffer is cleared on a jump (no stale echo from the old
/// position), and left alone on continuous frames.
#[test]
fn a_jump_zeroes_the_echo_buffer() {
    let mut e = LuaEngine::new();
    e.set_sources(&[(
        "main.lua",
        "dsp.echo = { delay = 15 }\n\
         function frame(t, f) if f == 3 then for a = 0x8800, 0xffff do aram[a] = 0x55 end end end",
    )])
    .unwrap();
    let marked = |e: &LuaEngine| e.aram()[0x8800..].iter().filter(|&&b| b == 0x55).count();
    for f in 0..6 {
        e.frame(f as f64 / 60.0, f).unwrap();
    }
    // The echo unit itself overwrites ~2 KB a frame; the rest is still there.
    assert!(marked(&e) > 20000, "continuous: {}", marked(&e));
    e.frame(100.0 / 60.0, 100).unwrap();
    assert_eq!(marked(&e), 0, "a jump clears it");
}

/// A jump keys the song's sounding voices off: what plays after it is only
/// the next note due, not a note held over from the old position.
#[test]
fn a_jump_keys_off_the_notes_sounding() {
    // One looping sine note held for 15 of 16 steps (ticks 0..469 of 500).
    let mut held = steps();
    held.rows = vec![Row {
        sound: "sine".into(),
        note: None,
        vol: 100,
        pan: 0,
    }];
    held.patterns[0].notes = vec![Note {
        at: 0,
        row: 0,
        len: 15 * 12,
        vel: 100,
        voice: None,
        nudge: 0,
        end_nudge: 0,
    }];
    let mut e = LuaEngine::new();
    common::add_sample(&mut e, "sine");
    common::add_song_source(&mut e, "held", &held);
    e.set_sources(&[("main.lua", "h = score{ song = \"held\" }")])
        .unwrap();
    let sounding = |e: &LuaEngine| e.dsp_view().voices.iter().any(|v| v.envx > 0);
    for f in 0..30 {
        e.frame(f as f64 / 60.0, f).unwrap();
    }
    assert!(sounding(&e), "the note sounds");
    // Frame 150: tick 624 = 500 + 124, inside the note on the second pass.
    for f in 150..155 {
        e.frame(f as f64 / 60.0, f).unwrap();
    }
    assert!(!sounding(&e), "keyed off until the next note due");
}
