//! Live reload of a playing song source: re-adding a `song`-kind source
//! under the same name (`add_source`) swaps the new song into every
//! `score{ song = "<name>" }` playing it, in place, keeping its musical
//! position (slot, 16th step within it, ticks into that step).
//! Driven through `add_source`/`set_sources`/`frame`/`score_view`/
//! `take_sram`/`audio`.

mod common;

use ppu_core::song::{encode, Note, Pattern, Row, Song};
use ppu_core::LuaEngine;
use serde_json::Value;
use std::time::{Duration, Instant};

/// `frames` counts frames in THIS VM: a full recompile starts it over, so
/// it tells the in-place path from the fallback. `kons` logs the tick at
/// which a frame first sees voice 0 keyed on (the native player writes a
/// fresh `vol` table per key-on); `koff` is the key-off mask pending when
/// `frame()` runs, i.e. what a reload just keyed off.
const MAIN: &str = "kons, frames, last = {}, 0, voice[0].vol\n\
     function frame()\n\
       frames = frames + 1\n\
       if voice[0].vol ~= last then kons[#kons + 1] = h.tick last = voice[0].vol end\n\
       sram.frames = frames sram.kons = kons sram.koff = __dsp_koff sram.playing = h.playing\n\
     end";

/// One "kick" row, patterns from kit-style step strings (`1`-`4` hit at
/// 32/64/96/127, `-` holds, `.` rests; each step a 16th = 12 units).
fn song(tempo: u32, swing: u32, patterns: &[(&str, &str)], arrangement: &[&str]) -> Song {
    let pattern = |(name, steps): &(&str, &str)| {
        let mut notes: Vec<Note> = Vec::new();
        for (i, c) in steps.chars().enumerate() {
            match c {
                '1'..='4' => notes.push(Note {
                    at: i as u32 * 12,
                    row: 0,
                    len: 12,
                    vel: [32, 64, 96, 127][c as usize - '1' as usize],
                    voice: None,
                    nudge: 0,
                    end_nudge: 0,
                }),
                '-' => notes.last_mut().unwrap().len += 12,
                _ => {}
            }
        }
        Pattern {
            name: name.to_string(),
            length: steps.len() as u32 * 12,
            tempo: None,
            notes,
        }
    };
    let index = |a: &&str| patterns.iter().position(|(n, _)| n == a).unwrap() as u32;
    Song {
        tempo: tempo * 100,
        swing,
        key: 0,
        voice_mask: 0xff,
        rows: vec![Row {
            sound: "kick".into(),
            note: None,
            vol: 127,
            pan: 0,
        }],
        patterns: patterns.iter().map(pattern).collect(),
        arrangement: arrangement.iter().map(index).collect(),
        loop_start: 0,
    }
}

fn beat(steps: &str) -> Song {
    song(120, 0, &[("A", steps)], &["A"])
}

fn payload(song: &Song) -> Vec<u8> {
    let mut p = vec![3u8, 5];
    p.extend_from_slice(&encode(song));
    p
}

fn push(e: &mut LuaEngine, song: &Song) {
    e.add_source("beat", &payload(song)).unwrap();
}

/// 120 BPM: a 16th is 31.25 ticks; 60 frames play 250 ticks.
fn start(song: &Song, looping: bool) -> LuaEngine {
    let mut e = LuaEngine::new();
    push(&mut e, song);
    let init = format!("function init() h = score{{ song = \"beat\", loop = {looping} }} end");
    e.set_sources(&[("main.lua", &format!("{MAIN}\n{init}"))])
        .unwrap();
    e
}

fn run(e: &mut LuaEngine, from: u32, to: u32) -> Value {
    for f in from..to {
        e.frame(f as f64 / 60.0, f).unwrap();
    }
    serde_json::from_str(&e.take_sram().expect("frame wrote sram")).unwrap()
}

#[test]
fn an_edit_keeps_the_tick_and_the_vm() {
    let mut e = start(&beat("4..............."), true);
    let before = run(&mut e, 0, 30);
    let at = e.score_view().unwrap();
    assert_eq!(at.song.as_deref(), Some("beat"));

    push(&mut e, &beat("4...4...4...4..."));
    let now = e.score_view().expect("still playing");
    assert_eq!((now.tick, now.length), (at.tick, 500), "the tick holds");

    let after = run(&mut e, 30, 31);
    assert_eq!(
        after["frames"],
        before["frames"].as_i64().unwrap() + 1,
        "same VM"
    );
    assert!(
        e.score_view().unwrap().tick > at.tick,
        "and it keeps playing"
    );
}

#[test]
fn the_cursor_reseeks_to_the_next_event_and_sounding_notes_key_off() {
    let mut e = start(&beat("4..............."), true);
    let before = run(&mut e, 0, 30); // tick ~125, between steps 4 and 5
    let seen = before["kons"].as_array().unwrap().len();
    assert_eq!(seen, 1, "the step-0 kick");
    assert_eq!(before["koff"], 0);

    // Step 2 (tick 63) is already behind the tick; step 8 (tick 250) is ahead.
    push(&mut e, &beat("4.4.....4......."));
    let first = run(&mut e, 30, 31);
    assert_eq!(first["koff"], 0xff, "the reload keys every voice off");
    let after = run(&mut e, 31, 62); // up to tick ~258
    let kons = &after["kons"].as_array().unwrap()[seen..];
    assert_eq!(kons.len(), 1, "only step 8 plays: {kons:?}");
    assert!((251..=255).contains(&kons[0].as_i64().unwrap()), "{kons:?}");
}

#[test]
fn a_sound_not_placed_at_setup_falls_back_to_a_recompile() {
    let mut e = start(&beat("4..............."), true);
    run(&mut e, 0, 30);
    let mut s = beat("4...4...........");
    s.rows[0].sound = "snare".into();
    push(&mut e, &s);
    let after = run(&mut e, 30, 31);
    assert_eq!(after["frames"], 1, "a fresh VM");
    // The recompiled score is anchored at t = 0 like the old one, so it
    // resumes where time says (frame 30 ends at tick ~129), not at tick 0.
    let tick = e.score_view().unwrap().tick;
    assert!((125..=130).contains(&tick), "resumes at t: {tick}");
}

/// A song that doesn't compile falls back to the recompile, which reports
/// it as the setup error it always was.
#[test]
fn a_bad_song_falls_back_to_the_recompile_that_reports_it() {
    let mut e = start(&beat("4..............."), true);
    run(&mut e, 0, 30);
    let mut s = beat("4...............");
    s.patterns[0].notes[0].nudge = -1; // starts before the song
    push(&mut e, &s);
    let err = e.frame(0.5, 30).unwrap_err();
    assert!(err.message.contains("song 'beat'"), "{}", err.message);
}

#[test]
fn a_length_shrinking_below_the_tick_wraps_or_stops() {
    for looping in [true, false] {
        let mut e = start(&beat("4..............."), looping);
        run(&mut e, 0, 90); // to tick ~375, then a length of 250
                            // The readout never shows a tick past the new end, even before a frame.
        push(&mut e, &beat("4......."));
        match e.score_view() {
            Some(v) if looping => assert_eq!((v.tick, v.length), (0, 250), "wrapped"),
            None if !looping => {}
            other => panic!("loop = {looping}: {other:?}"),
        }
        let after = run(&mut e, 90, 91);
        assert_eq!(after["frames"], 91, "in place");
        assert_eq!(after["playing"], looping);
    }
}

/// Every score playing the song reloads, not just the last one set up.
#[test]
fn every_score_playing_the_song_reloads() {
    let main = "function init() a = score{ song = \"beat\" } b = score{ song = \"beat\" } end\n\
                n = 0 function frame() n = n + 1 sram.n = n sram.a = a.length sram.b = b.length end";
    let mut e = LuaEngine::new();
    push(&mut e, &beat("4..............."));
    e.set_sources(&[("main.lua", main)]).unwrap();
    run(&mut e, 0, 10);
    push(&mut e, &beat("4......."));
    let after = run(&mut e, 10, 11);
    assert_eq!(after["n"], 11, "in place");
    assert_eq!(
        (after["a"].as_i64(), after["b"].as_i64()),
        (Some(250), Some(250))
    );
}

/// A song source added after setup recompiles, as any new source does.
#[test]
fn a_first_time_song_source_recompiles() {
    let mut e = start(&beat("4..............."), true);
    run(&mut e, 0, 30);
    e.add_source("fresh", &payload(&beat("4..."))).unwrap();
    assert_eq!(run(&mut e, 30, 31)["frames"], 1, "recompiled");
}

/// An update to a song source no score plays leaves the playing one alone:
/// same VM, same tick.
#[test]
fn an_edit_to_a_song_that_is_not_playing_keeps_the_playing_one() {
    let mut e = LuaEngine::new();
    push(&mut e, &beat("4..............."));
    e.add_source("other", &payload(&beat("4..."))).unwrap();
    let init = format!("{MAIN}\nfunction init() h = score{{ song = \"beat\" }} end");
    e.set_sources(&[("main.lua", &init)]).unwrap();
    let before = run(&mut e, 0, 30);
    let at = e.score_view().unwrap();

    e.add_source("other", &payload(&beat("4.4.4.4."))).unwrap();
    let now = e.score_view().unwrap();
    assert_eq!((now.tick, now.song.as_deref()), (at.tick, Some("beat")));
    let after = run(&mut e, 30, 31);
    assert_eq!(
        after["frames"],
        before["frames"].as_i64().unwrap() + 1,
        "same VM"
    );
    assert_eq!(after["koff"], 0, "nothing keyed off");
}

/// A reload that changes nothing (here: the same song with an unknown chunk
/// appended, so different bytes that decode equal) leaves the rendered audio
/// byte-identical to never reloading at all.
#[test]
fn a_reload_that_changes_nothing_keeps_the_audio_byte_identical() {
    let s = song(
        120,
        30,
        &[("A", "4.3-2...1.4-..3."), ("B", "..4...4.")],
        &["A", "B", "A"],
    );
    let mut same = payload(&s);
    same.extend_from_slice(b"XTRA\x02hi");
    let mut plain = start(&s, true);
    let mut reloaded = start(&s, true);
    let mut heard = false;
    for f in 0..300u32 {
        if f % 50 == 25 {
            reloaded.add_source("beat", &same).unwrap();
        }
        plain.frame(f as f64 / 60.0, f).unwrap();
        reloaded.frame(f as f64 / 60.0, f).unwrap();
        assert_eq!(plain.audio(), reloaded.audio(), "frame {f}");
        heard |= plain.audio().iter().any(|&s| s != 0);
    }
    assert!(heard, "the song never produced any audio");
}

/// kit.lua's step time: step `i` starts at this tick.
fn step_at(tempo: f64, swing: f64, i: i64) -> i64 {
    let s = if i % 2 == 1 {
        i as f64 + swing / 100.0
    } else {
        i as f64
    };
    (s * 3750.0 / tempo + 0.5).floor() as i64
}

fn timed(tempo: u32, swing: u32) -> Song {
    song(tempo, swing, &[("A", "4...............")], &["A"])
}

/// A tempo edit keeps the musical position: the same step, the same ticks
/// into it, not the same tick.
#[test]
fn a_tempo_change_keeps_the_step() {
    let mut e = start(&timed(120, 0), true);
    run(&mut e, 0, 62); // tick ~258: step 8 starts at 250
    let old = e.score_view().unwrap().tick;
    assert!((step_at(120.0, 0.0, 8)..step_at(120.0, 0.0, 9)).contains(&old));

    push(&mut e, &timed(60, 0));
    let now = e.score_view().unwrap();
    assert_eq!(now.length, 1000);
    assert_eq!(
        now.tick,
        step_at(60.0, 0.0, 8) + old - step_at(120.0, 0.0, 8)
    );
}

#[test]
fn a_swing_change_keeps_the_step() {
    let mut e = start(&timed(120, 0), true);
    run(&mut e, 0, 40); // tick ~166: odd step 5 starts at 156
    let old = e.score_view().unwrap().tick;
    assert!((step_at(120.0, 0.0, 5)..step_at(120.0, 0.0, 6)).contains(&old));

    push(&mut e, &timed(120, 50));
    let now = e.score_view().unwrap().tick;
    assert_eq!(now, step_at(120.0, 50.0, 5) + old - step_at(120.0, 0.0, 5));
    assert!((step_at(120.0, 50.0, 5)..step_at(120.0, 50.0, 6)).contains(&now));
}

/// One kick row at 120 BPM (a 16th is 31.25 ticks), 16-step patterns except
/// `A` (`a_steps`), in the given arrangement.
fn arranged(a_steps: usize, arrangement: &[&str]) -> Song {
    let a = format!("4{}", ".".repeat(a_steps - 1));
    let p = "4...............";
    song(120, 0, &[("A", &a), ("B", p), ("C", p)], arrangement)
}

/// The step (across the arrangement) that `tick` falls in at 120 BPM.
fn step_of(tick: i64) -> i64 {
    (0..).find(|&i| step_at(120.0, 0.0, i + 1) > tick).unwrap()
}

/// Shrinking an earlier pattern keeps the playhead in its arrangement slot,
/// at the same step within it, rather than at the same step overall.
#[test]
fn shrinking_an_earlier_pattern_keeps_the_slot_and_step() {
    let mut e = start(&arranged(16, &["A", "B", "B"]), true);
    run(&mut e, 0, 200); // tick ~833: slot 2 (steps 16..31), step 10
    let old = e.score_view().unwrap().tick;
    let i = step_of(old);
    assert!((16..32).contains(&i), "slot 2, step {i}");

    push(&mut e, &arranged(8, &["A", "B", "B"]));
    let now = e.score_view().unwrap();
    assert_eq!(now.length, step_at(120.0, 0.0, 40));
    assert_eq!(
        now.tick,
        step_at(120.0, 0.0, i - 8) + old - step_at(120.0, 0.0, i),
        "slot 2 now starts at step 8"
    );
}

/// Deleting a slot before the playhead keeps playing the same occurrence of
/// the pattern, now one slot earlier.
#[test]
fn deleting_an_earlier_slot_keeps_the_pattern_occurrence() {
    let mut e = start(&arranged(16, &["A", "B", "C"]), true);
    run(&mut e, 0, 264); // tick ~1100: slot 3 (steps 32..47)
    let old = e.score_view().unwrap().tick;
    assert!((32..48).contains(&step_of(old)));

    push(&mut e, &arranged(16, &["B", "C"]));
    let now = e.score_view().unwrap();
    assert_eq!((now.tick, now.length), (old - 500, 1000), "C, now slot 2");
}

/// A step past its slot's new length lands on that slot's end: the next
/// slot's first step.
#[test]
fn a_step_past_its_shrunk_slot_moves_to_the_next_slot() {
    let mut e = start(&arranged(16, &["A", "B"]), true);
    run(&mut e, 0, 80); // tick ~333: slot 1, step 10
    assert!((8..16).contains(&step_of(e.score_view().unwrap().tick)));

    push(&mut e, &arranged(8, &["A", "B"]));
    assert_eq!(e.score_view().unwrap().tick, step_at(120.0, 0.0, 8));
}

/// Deleting the slot the playhead is in (not the last one) wraps or stops,
/// like any other slot that's gone: it doesn't fall back to whatever slot
/// now shares its index.
#[test]
fn deleting_the_playing_slot_wraps_or_stops() {
    for looping in [true, false] {
        let mut e = start(&arranged(16, &["A", "C", "B"]), looping);
        run(&mut e, 0, 170); // tick ~708: slot 2 (C, steps 16..31)
        let old = e.score_view().unwrap().tick;
        assert!((16..32).contains(&step_of(old)));

        push(&mut e, &arranged(16, &["A", "B"]));
        match e.score_view() {
            Some(v) if looping => assert_eq!((v.tick, v.length), (0, 1000), "wrapped"),
            None if !looping => {}
            other => panic!("loop = {looping}: {other:?}"),
        }
        let after = run(&mut e, 170, 171);
        assert_eq!(after["playing"], looping);
    }
}

/// Adding or removing a slot after the playing one keeps playing that same
/// slot, not wrap or stop as if it had vanished.
#[test]
fn adding_or_removing_a_later_slot_keeps_the_playing_one() {
    let mut e = start(&arranged(16, &["A", "B", "C"]), true);
    run(&mut e, 0, 80); // tick ~333: slot 1 (A, steps 0..15)
    let old = e.score_view().unwrap().tick;
    assert!((0..16).contains(&step_of(old)));

    push(&mut e, &arranged(16, &["A", "B", "C", "B"]));
    let now = e.score_view().unwrap();
    assert_eq!((now.tick, now.length), (old, 2000), "appended slot");

    push(&mut e, &arranged(16, &["A", "B"]));
    let now = e.score_view().unwrap();
    assert_eq!(
        (now.tick, now.length),
        (old, 1000),
        "trailing slots dropped"
    );
}

/// Plays `from` (16-step slots, 500 ticks each) for `frames` frames, then
/// pushes `to`: the tick before, and the tick and length after.
fn moved(from: &[&str], frames: u32, to: &[&str]) -> (i64, i64, i64) {
    let mut e = start(&arranged(16, from), true);
    run(&mut e, 0, frames);
    let old = e.score_view().unwrap().tick;
    push(&mut e, &arranged(16, to));
    let now = e.score_view().unwrap();
    (old, now.tick, now.length)
}

/// Removing slots on both sides of the playhead in one write: the slots
/// left are the old ones in order, so the playing slot keeps playing at its
/// new position.
#[test]
fn removing_slots_on_both_sides_keeps_the_playing_one() {
    // tick ~1100: slot 3 (C, steps 32..47)
    let (old, now, len) = moved(&["A", "B", "C", "B"], 264, &["A", "C"]);
    assert!((32..48).contains(&step_of(old)));
    assert_eq!((now, len), (old - 500, 1000), "C, now slot 2");

    // The second A plays; B goes from both sides.
    let (old, now, len) = moved(&["A", "B", "A", "B"], 264, &["A", "A"]);
    assert_eq!((now, len), (old - 500, 1000), "second A, now slot 2");
}

/// Slots inserted on both sides of the playhead in one write keep the
/// playing slot too.
#[test]
fn inserting_slots_on_both_sides_keeps_the_playing_one() {
    let (old, now, len) = moved(&["A", "B", "C"], 264, &["A", "B", "B", "C", "A"]);
    assert!((32..48).contains(&step_of(old)));
    assert_eq!((now, len), (old + 500, 2500), "C, now slot 4");
}

/// Moving the playing slot one step earlier or later follows it: the
/// pattern keeps playing, not the neighbour that took its index.
#[test]
fn moving_the_playing_slot_follows_it() {
    // tick ~1100: slot 3 (C) moves earlier.
    let (old, now, len) = moved(&["A", "B", "C"], 264, &["A", "C", "B"]);
    assert_eq!((now, len), (old - 500, 1500), "C, now slot 2");

    // tick ~708: slot 2 (C) moves later.
    let (old, now, len) = moved(&["A", "C", "B"], 170, &["A", "B", "C"]);
    assert!((16..32).contains(&step_of(old)));
    assert_eq!((now, len), (old + 500, 1500), "C, now slot 3");
}

/// A tiny LCG: a deterministic stress song without a rand dep.
struct Lcg(u64);

impl Lcg {
    fn range(&mut self, n: u32) -> u32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1);
        ((self.0 >> 33) as u32) % n
    }
}

/// A 10-minute, 200 BPM song: 16 one-bar patterns of ~42 notes over 8
/// sample rows, played over a 500-slot arrangement (~21,000 notes).
fn stress_song() -> Song {
    let mut rng = Lcg(0x5EED);
    Song {
        tempo: 20000,
        swing: 0,
        key: 0,
        voice_mask: 0xff,
        rows: (0..8)
            .map(|i| Row {
                sound: format!("s{i}"),
                note: None,
                vol: 100,
                pan: 0,
            })
            .collect(),
        patterns: (0..16)
            .map(|i| Pattern {
                name: format!("bar{i}"),
                length: 192,
                tempo: None,
                notes: (0..42)
                    .map(|_| Note {
                        at: rng.range(192),
                        row: rng.range(8),
                        len: 12 + rng.range(37),
                        vel: 1 + rng.range(127),
                        voice: None,
                        nudge: 0,
                        end_nudge: 0,
                    })
                    .collect(),
            })
            .collect(),
        arrangement: (0..500).map(|i| i % 16).collect(),
        loop_start: 0,
    }
}

#[test]
fn stress_ten_minute_song_reloads_within_budget() {
    let mut e = LuaEngine::new();
    for i in 0..8 {
        common::add_sample(&mut e, &format!("s{i}"));
    }
    let base = stress_song();
    push(&mut e, &base);
    let main = "h = score{ song = \"beat\" }\nn = 0 function frame() n = n + 1 sram.n = n end";
    e.set_sources(&[("main.lua", main)]).unwrap();
    for f in 0..600 {
        e.frame(f as f64 / 60.0, f).unwrap();
    }

    let mut best = Duration::MAX;
    for n in 0..6u32 {
        let mut s = base.clone();
        s.patterns[3].notes[0].vel = 1 + n; // a real change every time
        let p = payload(&s);
        let t = Instant::now();
        e.add_source("beat", &p).unwrap();
        best = best.min(t.elapsed());
        let v = e.score_view().expect("still playing, in place");
        assert!(v.tick > 2400, "kept its position: {v:?}");
    }
    assert_eq!(run(&mut e, 600, 601)["n"], 601, "in place");

    println!("21k-note reload best of 6: {best:?}");
    if !cfg!(debug_assertions) {
        assert!(
            best < Duration::from_millis(10),
            "best {best:?} exceeded 10ms"
        );
    }
}
