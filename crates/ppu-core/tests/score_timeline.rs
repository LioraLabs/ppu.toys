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
        loop_start: 0,
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
       sram.heard = heard sram.playing = h.playing sram.f = f\n\
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
    static PITCHES: std::sync::OnceLock<Vec<i64>> = std::sync::OnceLock::new();
    let pitches = PITCHES.get_or_init(step_pitches);
    keys(sram)[skip..]
        .iter()
        .map(|k| (k["f"].as_u64().unwrap() as u32, k["p"].as_i64().unwrap()))
        .filter(|(f, _)| *f > after)
        .map(|(f, p)| (f, pitches.iter().position(|&q| q == p).expect("a step")))
        .collect()
}

fn log_len(sram: &Value) -> usize {
    keys(sram).len()
}

/// The key-on log (an empty Lua table serializes as `{}`).
fn keys(sram: &Value) -> &[Value] {
    sram["heard"].as_array().map_or(&[], |a| a)
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
    let v = e.score_view().expect("a finished song keeps its anchor");
    assert!(v.finished && v.anchor == Some(0.0) && v.tick == v.length);
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

/// `steps()` at another tempo: the same notes on a different timing.
fn steps_at(bpm: u32) -> Song {
    Song {
        tempo: bpm * 100,
        ..steps()
    }
}

/// `steps()` cut to its first `n` steps.
fn first_steps(n: usize) -> Song {
    let mut s = steps();
    s.patterns[0].length = n as u32 * 12;
    s.patterns[0].notes.truncate(n);
    s
}

/// A live reload in the second pass of a looping song anchored at 0.5 s
/// keeps its musical position: a timing edit moves the anchor by whole
/// ticks so the next note due is the remapped one.
#[test]
fn a_timing_reload_in_a_later_pass_keeps_the_position_and_rebases_the_anchor() {
    let mut e = engine("h = score{ song = \"steps\", at = 0.5 }");
    // Frame 240 starts at t = 4 s: tick 874 = 500 + 374, inside step 11.
    let seen = log_len(&run(&mut e, 0..240));
    common::add_song_source(&mut e, "steps", &steps_at(60));
    let sram = run(&mut e, 240..280);
    // Half the tempo: the rest of step 11 is twice as long, then step 12,
    // then step 13 a 16th (62.5 ticks = 15 frames) later.
    assert_eq!(heard(&sram, seen, 0)[..2], [(248, 12), (263, 13)], "{sram}");
    let anchor = e.score_view().unwrap().anchor.unwrap();
    let moved = (anchor - 0.5) * 250.0;
    assert!(moved != 0.0 && moved.fract() == 0.0, "whole ticks: {moved}");
}

#[test]
fn a_note_only_reload_keeps_the_anchor_exactly() {
    let mut e = engine("h = score{ song = \"steps\", at = 0.5 }");
    let seen = log_len(&run(&mut e, 0..240));
    let mut quieter = steps();
    for n in &mut quieter.patterns[0].notes {
        n.vel = 50;
    }
    common::add_song_source(&mut e, "steps", &quieter);
    let sram = run(&mut e, 240..250);
    assert_eq!(e.score_view().unwrap().anchor, Some(0.5));
    // Step 12 starts at tick 375 of the pass, due at 4 s + 4 ms.
    assert_eq!(heard(&sram, seen, 0).first(), Some(&(241, 12)), "{sram}");
}

/// After a jump and before the song's next fire, there is no position to
/// keep: the reload leaves the anchor, and time decides where it resumes.
#[test]
fn a_reload_right_after_a_jump_resumes_by_time() {
    let mut e = engine("h = score{ song = \"steps\", at = 2 }");
    run(&mut e, 0..10);
    // Jump to frame 30 (still before the anchor: no fire yet).
    let seen = log_len(&run(&mut e, 30..31));
    common::add_song_source(&mut e, "steps", &steps_at(60));
    assert_eq!(e.score_view().unwrap().anchor, Some(2.0));
    let sram = run(&mut e, 31..140);
    assert_eq!(heard(&sram, seen, 0)[..2], [(121, 0), (136, 1)], "{sram}");
}

#[test]
fn a_reload_of_a_finished_loop_false_song_keeps_it_silent() {
    let mut e = engine("h = score{ song = \"steps\", loop = false }");
    // Over at 2 s (frame 120).
    let seen = log_len(&run(&mut e, 0..150));
    // Twice as long: the old end is now mid-song, but it stays finished.
    common::add_song_source(&mut e, "steps", &steps_at(60));
    let sram = run(&mut e, 150..300);
    assert_eq!(heard(&sram, seen, 0), vec![], "{sram}");
    assert_eq!(sram["playing"], false);
    assert!(e.score_view().unwrap().finished);
}

/// A reload that cuts the song short of where it is: a loop wraps to the
/// top of its next pass, a loop = false song finishes.
#[test]
fn a_reload_past_the_new_end_wraps_or_finishes() {
    let mut e = engine("h = score{ song = \"steps\" }");
    // Frame 100: tick 416, step 13 — past an 8-step song's end.
    let seen = log_len(&run(&mut e, 0..100));
    common::add_song_source(&mut e, "steps", &first_steps(8));
    let sram = run(&mut e, 100..110);
    assert_eq!(heard(&sram, seen, 0)[..2], [(101, 0), (108, 1)], "{sram}");

    let mut e = engine("h = score{ song = \"steps\", loop = false }");
    let seen = log_len(&run(&mut e, 0..100));
    common::add_song_source(&mut e, "steps", &first_steps(8));
    assert!(
        e.score_view().unwrap().finished,
        "finished by the reload itself"
    );
    let sram = run(&mut e, 100..200);
    assert_eq!(heard(&sram, seen, 0), vec![], "{sram}");
    assert_eq!(sram["playing"], false);
}

/// A song a `play()` started (the program declares `at = false`) is not
/// restarted or dropped by a recompile: it keeps playing at t.
#[test]
fn a_recompile_keeps_a_played_song_playing_at_t() {
    let src = "h = score{ song = \"steps\", at = false }\n\
               function body(f) if f == 10 then h.play() end end";
    let mut e = engine(src);
    run(&mut e, 0..40);
    let anchor = e.score_view().unwrap().anchor;
    e.set_sources(&[("main.lua", &program(src))]).unwrap();
    assert_eq!(e.score_view().map(|v| v.anchor), Some(anchor));
    // Frame 10 starts at sample 5333 (0.1667 s); frame 40 at 0.6667 s:
    // tick 125 into the song, where step 4 starts.
    let sram = run(&mut e, 40..50);
    assert_eq!(heard(&sram, 0, 0).first(), Some(&(41, 4)), "{sram}");
}

/// One looping sine note held for 15 of 16 steps (ticks 0..469 of 500).
fn held_engine(main: &str) -> LuaEngine {
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
    e.set_sources(&[("main.lua", main)]).unwrap();
    e
}

fn sounding(e: &LuaEngine) -> bool {
    e.dsp_view().voices.iter().any(|v| v.envx > 0)
}

#[test]
fn a_recompile_that_drops_the_score_keys_its_voices_off() {
    let mut e = held_engine("h = score{ song = \"held\" }");
    for f in 0..30 {
        e.frame(f as f64 / 60.0, f).unwrap();
    }
    assert!(sounding(&e), "the note sounds");
    e.set_sources(&[("main.lua", "function frame() end")])
        .unwrap();
    for f in 30..35 {
        e.frame(f as f64 / 60.0, f).unwrap();
    }
    assert!(!sounding(&e), "keyed off");
    assert_eq!(e.score_view(), None);
}

#[test]
fn a_recompile_does_not_cut_a_continuing_song() {
    let main = "h = score{ song = \"held\" }";
    let mut e = held_engine(main);
    for f in 0..30 {
        e.frame(f as f64 / 60.0, f).unwrap();
    }
    e.set_sources(&[("main.lua", &format!("{main} -- edited"))])
        .unwrap();
    for f in 30..35 {
        e.frame(f as f64 / 60.0, f).unwrap();
    }
    assert!(sounding(&e), "still sounding mid-note");
}

/// Run is a power cycle: a `play()`-started song does not survive it (the
/// `sram` guard keeps the replayed program from calling `play()` again),
/// and a declared anchor is restored from the top.
#[test]
fn reset_drops_a_played_song_and_restores_a_declared_anchor() {
    let mut e = engine(
        "h = score{ song = \"steps\", at = false }\n\
         function body(f) if f == 10 and not sram.played then h.play() sram.played = true end end",
    );
    assert!(log_len(&run(&mut e, 0..40)) > 0, "played before the reset");
    e.reset().unwrap();
    let sram = run(&mut e, 0..60);
    assert_eq!(heard(&sram, 0, 0), vec![], "{sram}");
    assert_eq!(e.score_view(), None);

    let mut e = engine("h = score{ song = \"steps\", at = 0 }");
    run(&mut e, 0..90);
    e.reset().unwrap();
    let sram = run(&mut e, 0..10);
    assert_eq!(heard(&sram, 0, 0).first(), Some(&(1, 0)), "{sram}");
    assert_eq!(e.score_view().unwrap().anchor, Some(0.0));
}

/// Removing an earlier `score{}` shifts the indices: the song that moved
/// keeps sounding because an anchored new player still plays it.
#[test]
fn a_recompile_that_shifts_a_continuing_song_does_not_cut_it() {
    let mut e = held_engine(
        "x = score{ song = \"held\", at = false }\n\
         h = score{ song = \"held\" }",
    );
    for f in 0..30 {
        e.frame(f as f64 / 60.0, f).unwrap();
    }
    assert!(sounding(&e), "the note sounds");
    e.set_sources(&[("main.lua", "h = score{ song = \"held\" }")])
        .unwrap();
    for f in 30..35 {
        e.frame(f as f64 / 60.0, f).unwrap();
    }
    assert!(sounding(&e), "still sounding mid-note");
}

/// Only a declared `at = false` takes a played song over: `stop()` in
/// `init()` means stopped.
#[test]
fn a_recompile_that_stops_the_song_in_init_does_not_carry_it() {
    let played = "h = score{ song = \"steps\", at = false }\n\
                  function body(f) if f == 10 then h.play() end end";
    let mut e = engine(played);
    assert!(
        log_len(&run(&mut e, 0..40)) > 0,
        "played before the recompile"
    );
    let stopped = format!("{played}\nfunction init() h.stop() end");
    e.set_sources(&[("main.lua", &program(&stopped))]).unwrap();
    assert_eq!(e.score_view(), None);
    let sram = run(&mut e, 40..60);
    assert_eq!(heard(&sram, 0, 0), vec![], "{sram}");
}

/// Editing `at = 0` into `at = false` is a declared `at = false`: the song
/// that was sounding keeps its anchor (game music resumes across edits).
#[test]
fn a_recompile_from_at_0_to_at_false_keeps_the_song() {
    let mut e = engine("h = score{ song = \"steps\", at = 0 }");
    run(&mut e, 0..90);
    let edited = program("h = score{ song = \"steps\", at = false }");
    e.set_sources(&[("main.lua", &edited)]).unwrap();
    let sram = run(&mut e, 90..100);
    // Frame 90: t = 1.5 s, tick 374; step 12 starts at 375.
    assert_eq!(heard(&sram, 0, 0).first(), Some(&(91, 12)), "{sram}");
}

/// `__scores` is a plain global: a program that clobbers it still
/// recompiles, and the played song still carries.
#[test]
fn a_recompile_survives_a_clobbered_scores_table() {
    let src = "h = score{ song = \"steps\", at = false }\n\
               __scores = nil\n\
               function body(f) if f == 10 then h.play() end end";
    let mut e = engine(src);
    run(&mut e, 0..40);
    e.set_sources(&[("main.lua", &program(src))]).unwrap();
    let sram = run(&mut e, 40..50);
    assert_eq!(heard(&sram, 0, 0).first(), Some(&(41, 4)), "{sram}");
}

/// A re-anchored player of the same song is a restart, not a continuation:
/// the old song's voices key off (the new one resumes at its next note due).
#[test]
fn a_recompile_that_re_anchors_the_song_keys_it_off() {
    let mut e = held_engine("h = score{ song = \"held\" }");
    for f in 0..30 {
        e.frame(f as f64 / 60.0, f).unwrap();
    }
    assert!(sounding(&e), "the note sounds");
    e.set_sources(&[("main.lua", "h = score{ song = \"held\", at = 0.25 }")])
        .unwrap();
    for f in 30..35 {
        e.frame(f as f64 / 60.0, f).unwrap();
    }
    assert!(!sounding(&e), "keyed off until the next note due");
}

// ---- a recompile while paused: the next frame(t, last_f) is a replay ----

/// Paused at f = 5 (frames 0..=5 rendered, then a repeat of 5), the edit
/// adds an `init()` that keys voice 0 on. The studio's paused refresh
/// renders f = 5 again: that replay delivers the key-on.
#[test]
fn a_paused_recompile_delivers_inits_key_on() {
    let mut e = LuaEngine::new();
    e.set_sources(&[("main.lua", "function frame() end")])
        .unwrap();
    for f in 0..6 {
        e.frame(f as f64 / 60.0, f).unwrap();
    }
    e.frame(5.0 / 60.0, 5).unwrap();
    assert!(e.audio().is_empty(), "paused: a repeat is inert");
    e.set_sources(&[(
        "main.lua",
        "piano = bank('piano')\n\
         function init()\n\
           voice[0].sample = piano.sample\n\
           voice[0].pitch = 0x1000\n\
           voice[0].vol = { l = 127, r = 127 }\n\
           voice[0].adsr = { a = 15, d = 0, s = 7, r = 0 }\n\
           dsp.mvol = { l = 127, r = 127 }\n\
           kon(0)\n\
         end\n\
         function frame() end",
    )])
    .unwrap();
    e.frame(5.0 / 60.0, 5).unwrap();
    assert!(!e.audio().is_empty(), "the replay renders span 5");
    e.frame(5.0 / 60.0, 5).unwrap();
    assert!(
        e.audio().is_empty(),
        "only the first frame after it replays"
    );
    let mut energy = 0i64;
    for f in 6..12 {
        e.frame(f as f64 / 60.0, f).unwrap();
        energy += e.audio().iter().map(|&s| (s as i64).abs()).sum::<i64>();
    }
    assert!(e.dsp_view().voices[0].envx > 0, "voice 0 sounds");
    assert!(energy > 0, "and is heard");
}

/// A `score{}` anchored at 0 added while paused at f = 0 plays its
/// downbeat: the replay of span 0 holds ticks 0..=3.
#[test]
fn a_score_added_while_paused_at_0_plays_tick_0() {
    let mut e = engine("h = {}");
    run(&mut e, 0..1);
    e.set_sources(&[(
        "main.lua",
        &program("h = score{ song = \"steps\", at = 0 }"),
    )])
    .unwrap();
    let sram = run(&mut e, 0..5);
    assert_eq!(heard(&sram, 0, 0).first(), Some(&(1, 0)), "{sram}");
}

/// A paused edit that removes the `score{}`: the replay delivers the
/// recompile's key-off, so the dropped song's voices stop.
#[test]
fn a_paused_recompile_that_drops_the_score_keys_its_voices_off() {
    let mut e = held_engine("h = score{ song = \"held\" }");
    for f in 0..30 {
        e.frame(f as f64 / 60.0, f).unwrap();
    }
    e.frame(29.0 / 60.0, 29).unwrap();
    assert!(sounding(&e), "the note sounds");
    let before = e.dsp_view().voices[0].envx;
    e.set_sources(&[("main.lua", "function frame() end")])
        .unwrap();
    // The replayed frame itself delivers the key-off: the envelope is
    // already releasing when it returns, before frame 30 is rendered.
    e.frame(29.0 / 60.0, 29).unwrap();
    let after = e.dsp_view().voices[0].envx;
    assert!(
        after < before,
        "released by the replay: {before} -> {after}"
    );
    for f in 30..35 {
        e.frame(f as f64 / 60.0, f).unwrap();
    }
    assert!(!sounding(&e), "keyed off");
}

/// The paused twin of `a_recompile_does_not_cut_a_continuing_song`: the
/// replay is not a jump, so an edited program that keeps the score leaves
/// the held note sounding and the echo region alone.
#[test]
fn a_paused_recompile_that_keeps_the_score_is_not_a_jump() {
    let main = "dsp.echo = { delay = 15 }\nh = score{ song = \"held\" }";
    // A recompile resets ARAM, so the replayed frame's own body marks the
    // echo region; a jump would zero it again after the offset-0 flush.
    let edited =
        format!("{main}\nfunction frame(t, f) for a = 0x8800, 0xffff do aram[a] = 0x55 end end");
    let mut e = held_engine(main);
    for f in 0..30 {
        e.frame(f as f64 / 60.0, f).unwrap();
    }
    e.frame(29.0 / 60.0, 29).unwrap();
    e.set_sources(&[("main.lua", &edited)]).unwrap();
    e.frame(29.0 / 60.0, 29).unwrap();
    let marked = e.aram()[0x8800..].iter().filter(|&&b| b == 0x55).count();
    assert!(sounding(&e), "the held note is still sounding");
    assert!(marked > 20000, "echo region kept: {marked}");
}

/// Three 4-step slots (A, B, C; 125 ticks each, 375 in all), step `i` of the
/// song its own row so a pitch names the step; the loop restarts at slot
/// `loop_start`.
fn arranged_steps(loop_start: u32) -> Song {
    let mut s = steps();
    s.rows.truncate(12);
    s.patterns = (0..3)
        .map(|k| Pattern {
            name: ["A", "B", "C"][k as usize].into(),
            length: 4 * 12,
            tempo: None,
            notes: (0..4)
                .map(|j| Note {
                    at: j * 12,
                    row: 4 * k + j,
                    len: 12,
                    vel: 100,
                    voice: None,
                    nudge: 0,
                    end_nudge: 0,
                })
                .collect(),
        })
        .collect();
    s.arrangement = vec![0, 1, 2];
    s.loop_start = loop_start;
    s
}

fn engine_of(song: &Song, score: &str) -> LuaEngine {
    let mut e = LuaEngine::new();
    common::add_song_source(&mut e, "steps", song);
    e.set_sources(&[("main.lua", &program(score))]).unwrap();
    e
}

fn played(sram: &Value) -> Vec<usize> {
    heard(sram, 0, 0).into_iter().map(|(_, s)| s).collect()
}

#[test]
fn the_intro_plays_once_then_the_loop_restarts_at_the_loop_start_slot() {
    let mut e = engine_of(&arranged_steps(1), "h = score{ song = \"steps\" }");
    let sram = run(&mut e, 0..125);
    let want: Vec<usize> = (0..12).chain(4..8).collect();
    assert_eq!(played(&sram)[..16], want[..], "{sram}");
}

#[test]
fn a_seek_into_a_later_pass_lands_in_the_loop_body() {
    let mut e = engine_of(&arranged_steps(1), "h = score{ song = \"steps\" }");
    run(&mut e, 0..10);
    // Frame 200: tick 833 = 375 + 250 + 208, so 333 in the body: step 10.
    let seen = log_len(&run(&mut e, 200..201));
    let v = e.score_view().unwrap();
    assert_eq!(
        (v.slot, v.step, v.loop_tick),
        (Some(2), Some(10), Some(125))
    );
    assert!((313..344).contains(&v.tick), "{v:?}");
    let sram = run(&mut e, 201..210);
    assert_eq!(heard(&sram, seen, 200).first().map(|k| k.1), Some(11));
}

#[test]
fn the_view_wraps_to_the_loop_tick_not_zero() {
    let mut e = engine_of(&arranged_steps(1), "h = score{ song = \"steps\" }");
    // The first fire of the second pass is tick 125 (slot 1, step 4).
    run(&mut e, 0..91);
    let v = e.score_view().unwrap();
    assert!((125..140).contains(&v.tick), "{v:?}");
    assert_eq!(v.slot, Some(1));
}

#[test]
fn loop_false_ignores_the_loop_start() {
    let mut e = engine_of(
        &arranged_steps(1),
        "h = score{ song = \"steps\", loop = false }",
    );
    let sram = run(&mut e, 0..150);
    assert_eq!(played(&sram), (0..12).collect::<Vec<_>>(), "{sram}");
    assert_eq!(sram["playing"], false);
    assert_eq!(e.score_view().unwrap().loop_tick, None);
}

#[test]
fn a_finished_loop_false_song_keeps_its_anchor_and_a_jump_back_unfinishes_it() {
    let mut e = engine("h = score{ song = \"steps\", loop = false, at = 0.25 }");
    run(&mut e, 0..200);
    let v = e.score_view().expect("finished, not gone");
    assert!(
        v.finished && v.anchor == Some(0.25) && v.tick == v.length,
        "{v:?}"
    );
    assert_eq!((v.slot, v.step), (None, None));
    run(&mut e, 60..70);
    let v = e.score_view().unwrap();
    assert!(!v.finished && v.anchor == Some(0.25), "{v:?}");
}

/// Plays `steps()` (2 s) from f = 0 as far as `edit_f`, applies `edit`, runs
/// on a little, then jumps back to f = 0: the frame (and step) the song's
/// first key-on lands in.
fn first_on_after_rewind(score: &str, edit_f: u32, edit: &Song) -> (u32, usize) {
    let mut e = engine(score);
    run(&mut e, 0..edit_f);
    common::add_song_source(&mut e, "steps", edit);
    run(&mut e, edit_f..edit_f + 10);
    let seen = log_len(&run(&mut e, edit_f + 10..edit_f + 11));
    let sram = run(&mut e, 0..30);
    heard(&sram, seen, 0)[0]
}

/// A live edit keeps musical position by shifting the anchor; the next jump
/// must go back to the declared one.
#[test]
fn rewinding_a_finished_song_after_extending_it_plays_from_the_top() {
    let mut longer = steps();
    longer.arrangement = vec![0; 4];
    // t = 2.5 s, the 2 s song long finished.
    let got = first_on_after_rewind("h = score{ song = \"steps\", loop = false }", 150, &longer);
    assert_eq!(got, (1, 0));
}

#[test]
fn rewinding_a_looping_song_after_a_double_tempo_edit_has_no_silence() {
    let got = first_on_after_rewind("h = score{ song = \"steps\" }", 240, &steps_at(240));
    assert_eq!(got, (1, 0));
}

#[test]
fn rewinding_a_looping_song_after_a_half_tempo_edit_keeps_bar_one_on_time() {
    let unedited = first_on_after_rewind("h = score{ song = \"steps\" }", 100, &steps());
    let got = first_on_after_rewind("h = score{ song = \"steps\" }", 100, &steps_at(60));
    assert_eq!(got, unedited);
    assert_eq!(got, (1, 0));
}
