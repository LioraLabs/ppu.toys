//! Hot reload of a playing song: a push that changes only a song file
//! (`seq_<id>` and nothing else) re-runs it in the live VM, and every
//! `score{ song = "<id>" }` recompiles in place, keeping its step (its
//! position in the song). Driven
//! through `set_sources`/`frame`/`score_view`/`take_sram`.

use ppu_core::LuaEngine;
use serde_json::Value;

/// `frames` counts frames in THIS VM: a full recompile starts it over, so
/// it tells the in-place path from the fallback. `kons` logs the tick of
/// every key-on, `koffs` counts key-offs.
const MAIN: &str = "local real_kon, real_koff = kon, koff\n\
     kons, koffs, frames = {}, 0, 0\n\
     function kon(v) kons[#kons + 1] = __score.tick real_kon(v) end\n\
     function koff(v) koffs = koffs + 1 real_koff(v) end\n\
     function frame()\n\
       frames = frames + 1\n\
       sram.frames = frames sram.kons = kons sram.koffs = koffs sram.playing = h.playing\n\
     end";

fn beat(step_strings: &str) -> String {
    beat_with(step_strings, "{ sound = \"kick\" }")
}

fn beat_with(step_strings: &str, rows: &str) -> String {
    format!(
        "function seq_beat() return {{\n  tempo = 120,\n  rows = {{ {rows} }},\n  \
         patterns = {{ A = {{ {step_strings} }} }},\n  arrangement = {{ \"A\" }},\n}} end\n"
    )
}

/// 120 BPM: a 16th is 31.25 ticks; 60 frames play 250 ticks.
fn start(song: &str, looping: bool) -> LuaEngine {
    let mut e = LuaEngine::new();
    let init = format!("function init() h = score{{ song = \"beat\", loop = {looping} }} end");
    e.set_sources(&[("beat.lua", song), ("main.lua", &format!("{MAIN}\n{init}"))])
        .unwrap();
    e
}

fn push(e: &mut LuaEngine, song: &str, looping: bool) -> Result<(), ppu_core::LuaError> {
    let init = format!("function init() h = score{{ song = \"beat\", loop = {looping} }} end");
    e.set_sources(&[("beat.lua", song), ("main.lua", &format!("{MAIN}\n{init}"))])
}

fn run(e: &mut LuaEngine, from: u32, to: u32) -> Value {
    for f in from..to {
        e.frame(f as f64 / 60.0, f).unwrap();
    }
    serde_json::from_str(&e.take_sram().expect("frame wrote sram")).unwrap()
}

#[test]
fn an_edit_keeps_the_tick_and_the_vm() {
    let mut e = start(&beat("\"4...............\""), true);
    let before = run(&mut e, 0, 30);
    let at = e.score_view().unwrap();
    assert_eq!(at.song.as_deref(), Some("beat"));

    push(&mut e, &beat("\"4...4...4...4...\""), true).unwrap();
    let now = e.score_view().expect("still playing");
    assert_eq!((now.tick, now.length), (at.tick, 500), "the tick holds");
    assert_eq!(now.song.as_deref(), Some("beat"));

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
    let mut e = start(&beat("\"4...............\""), true);
    let before = run(&mut e, 0, 30); // tick ~125, between steps 4 and 5
    assert_eq!(before["kons"], serde_json::json!([0]));
    let koffs = before["koffs"].as_i64().unwrap();

    // Step 2 (tick 63) is already behind the tick; step 8 (tick 250) is ahead.
    push(&mut e, &beat("\"4.4.....4.......\""), true).unwrap();
    let after = run(&mut e, 30, 62); // up to tick ~258, before that kick ends
    assert_eq!(after["kons"], serde_json::json!([0, 250]));
    assert_eq!(
        after["koffs"].as_i64().unwrap() - koffs,
        8,
        "the reload keys every voice off"
    );
}

#[test]
fn a_sound_not_placed_at_setup_falls_back_to_a_recompile() {
    let mut e = start(&beat("\"4...............\""), true);
    run(&mut e, 0, 30);
    push(
        &mut e,
        &beat_with(
            "\"4...............\", \"....4...........\"",
            "{ sound = \"kick\" }, { sound = \"snare\" }",
        ),
        true,
    )
    .unwrap();
    assert_eq!(e.score_view().unwrap().tick, 0, "a fresh score");
    let after = run(&mut e, 30, 31);
    assert_eq!(after["frames"], 1, "a fresh VM");
}

#[test]
fn a_bad_edit_keeps_the_old_song_playing_and_names_the_song_file() {
    let mut e = start(&beat("\"4...............\""), true);
    run(&mut e, 0, 30);
    let at = e.score_view().unwrap();

    let err = push(&mut e, &beat("\"4...x...........\""), true).unwrap_err();
    assert_eq!(err.file.as_deref(), Some("beat.lua"), "{err:?}");
    assert!(err.message.contains("bad 'x'"), "{}", err.message);
    let now = e.score_view().unwrap();
    assert_eq!(
        (now.tick, now.length),
        (at.tick, 500),
        "the old events stand"
    );

    let after = run(&mut e, 30, 31);
    assert_eq!(after["frames"], 31, "the same VM keeps running");
    assert!(e.score_view().unwrap().tick > at.tick);
}

#[test]
fn a_length_shrinking_below_the_tick_wraps_or_stops() {
    for looping in [true, false] {
        let mut e = start(&beat("\"4...............\""), looping);
        // To tick ~375, then a length of 250.
        run(&mut e, 0, 90);
        // The readout never shows a tick past the new end, even before a frame.
        push(&mut e, &beat("\"4.......\""), looping).unwrap();
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

/// A `data =` score doesn't reload, so an edit to its song recompiles the
/// toy rather than being dropped.
#[test]
fn an_edit_to_a_data_song_recompiles() {
    let mut e = LuaEngine::new();
    let files = |song: &str| {
        [
            ("beat.lua", song.to_string()),
            (
                "main.lua",
                format!("{MAIN}\nfunction init() h = score{{ data = seq_beat() }} end"),
            ),
        ]
    };
    let push = |e: &mut LuaEngine, song: &str| {
        let f = files(song);
        e.set_sources(&[("beat.lua", &f[0].1), ("main.lua", &f[1].1)])
    };
    push(&mut e, &beat("\"4...............\"")).unwrap();
    run(&mut e, 0, 30);
    push(&mut e, &beat("\"4...4...4...4...\"")).unwrap();
    assert_eq!(run(&mut e, 30, 31)["frames"], 1, "recompiled");
}

/// A push reloads all its songs or none: when one song's new data is bad,
/// the other song's edit isn't half-applied in the running VM.
#[test]
fn a_push_reloads_every_song_or_none() {
    let song = |id: &str, rows: &str, steps: &str| {
        format!(
            "function seq_{id}() return {{ tempo = 120, rows = {{ {rows} }},\n  \
             patterns = {{ A = {{ {steps} }} }}, arrangement = {{ \"A\" }} }} end\n"
        )
    };
    let main = "function init() a = score{ song = \"a\" } b = score{ song = \"b\" } end\n\
                function frame(t, f) sram.f = f sram.a = #a.events end";
    let kick = "{ sound = \"kick\" }";
    let mut e = LuaEngine::new();
    e.set_sources(&[
        ("a.lua", &song("a", kick, "\"4...............\"")),
        ("b.lua", &song("b", kick, "\"4...............\"")),
        ("main.lua", main),
    ])
    .unwrap();
    run(&mut e, 0, 10);
    let err = e
        .set_sources(&[
            ("a.lua", &song("a", kick, "\"4...4...4...4...\"")),
            ("b.lua", &song("b", kick, "\"4...x...........\"")),
            ("main.lua", main),
        ])
        .unwrap_err();
    assert_eq!(err.file.as_deref(), Some("b.lua"), "{err:?}");
    assert_eq!(run(&mut e, 10, 11)["a"], 1, "a keeps its old events");
}

#[test]
fn a_file_with_other_top_level_code_is_not_a_song_file() {
    let mut e = start(&beat("\"4...............\""), true);
    run(&mut e, 0, 30);
    let song = format!("{}extra = 1\n", beat("\"4...4...........\""));
    push(&mut e, &song, true).unwrap();
    assert_eq!(run(&mut e, 30, 31)["frames"], 1, "recompiled");
}

/// A pushed file that wasn't there before: a new song file (its id not
/// defined elsewhere) is live, since nothing can be playing it by name. It
/// recompiles when it isn't a song file, when its id is already taken, or
/// while a `data =` score is set up (that table may be any song's).
#[test]
fn an_added_song_file_is_live_unless_a_data_score_is_set_up() {
    let other = "function seq_other() return {} end\n";
    let with = |main: &str, name: &str, added: &str| {
        let mut e = LuaEngine::new();
        let song = beat("\"4...............\"");
        e.set_sources(&[("beat.lua", &song), ("main.lua", main)])
            .unwrap();
        let before = run(&mut e, 0, 30);
        let at = e.score_view().unwrap().tick;
        e.set_sources(&[("beat.lua", &song), (name, added), ("main.lua", main)])
            .unwrap();
        let tick = e.score_view().unwrap().tick;
        let frames = run(&mut e, 30, 31)["frames"].as_i64().unwrap();
        (frames == before["frames"].as_i64().unwrap() + 1, tick == at)
    };
    let by_name = format!("{MAIN}\nfunction init() h = score{{ song = \"beat\" }} end");
    let by_data = format!("{MAIN}\nfunction init() h = score{{ data = seq_beat() }} end");

    assert_eq!(with(&by_name, "other.lua", other), (true, true), "live");
    assert_eq!(with(&by_data, "other.lua", other).0, false, "data = live");
    assert_eq!(
        with(&by_name, "other.lua", "x = 1\n").0,
        false,
        "not a song"
    );
    let again = beat("\"4...4...........\"");
    assert_eq!(with(&by_name, "again.lua", &again).0, false, "id taken");
}

/// An edit to a song no score is bound to (and no `data =` score could be
/// built from) is live: the playing song keeps its VM and its tick.
#[test]
fn an_edit_to_a_song_that_is_not_playing_keeps_the_playing_one() {
    let other = |steps: &str| {
        format!(
            "function seq_other() return {{ tempo = 90, rows = {{ {{ sound = \"snare\" }} }},\n  \
             patterns = {{ A = {{ {steps} }} }}, arrangement = {{ \"A\" }} }} end\n"
        )
    };
    let init = format!("{MAIN}\nfunction init() h = score{{ song = \"beat\" }} end");
    let song = beat("\"4...............\"");
    let mut e = LuaEngine::new();
    e.set_sources(&[
        ("beat.lua", &song),
        ("other.lua", &other("\"4...............\"")),
        ("main.lua", &init),
    ])
    .unwrap();
    let before = run(&mut e, 0, 30);
    let at = e.score_view().unwrap();

    e.set_sources(&[
        ("beat.lua", &song),
        ("other.lua", &other("\"4...4...4...4...\"")),
        ("main.lua", &init),
    ])
    .unwrap();
    let now = e.score_view().unwrap();
    assert_eq!((now.tick, now.song.as_deref()), (at.tick, Some("beat")));
    assert_eq!(
        run(&mut e, 30, 31)["frames"],
        before["frames"].as_i64().unwrap() + 1,
        "same VM"
    );
}

fn timed(tempo: u32, swing: u32) -> String {
    format!(
        "function seq_beat() return {{ tempo = {tempo}, swing = {swing},\n  \
         rows = {{ {{ sound = \"kick\" }} }}, patterns = {{ A = {{ \"4...............\" }} }},\n  \
         arrangement = {{ \"A\" }} }} end\n"
    )
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

/// A tempo edit keeps the musical position: the same step, the same ticks
/// into it, not the same tick.
#[test]
fn a_tempo_change_keeps_the_step() {
    let mut e = start(&timed(120, 0), true);
    run(&mut e, 0, 62); // tick ~258: step 8 starts at 250
    let old = e.score_view().unwrap().tick;
    assert!((step_at(120.0, 0.0, 8)..step_at(120.0, 0.0, 9)).contains(&old));

    push(&mut e, &timed(60, 0), true).unwrap();
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

    push(&mut e, &timed(120, 50), true).unwrap();
    let now = e.score_view().unwrap().tick;
    assert_eq!(now, step_at(120.0, 50.0, 5) + old - step_at(120.0, 0.0, 5));
    assert!((step_at(120.0, 50.0, 5)..step_at(120.0, 50.0, 6)).contains(&now));
}

/// One kick row at 120 BPM (a 16th is 31.25 ticks), 16-step patterns except
/// `A` (`a_steps`), in the given arrangement.
fn arranged(a_steps: usize, arrangement: &str) -> String {
    let a = format!("\"4{}\"", ".".repeat(a_steps - 1));
    let p = "\"4...............\"";
    format!(
        "function seq_beat() return {{ tempo = 120, rows = {{ {{ sound = \"kick\" }} }},\n  \
         patterns = {{ A = {{ {a} }}, B = {{ {p} }}, C = {{ {p} }} }},\n  \
         arrangement = {{ {arrangement} }} }} end\n"
    )
}

/// The step (across the arrangement) that `tick` falls in at 120 BPM.
fn step_of(tick: i64) -> i64 {
    (0..).find(|&i| step_at(120.0, 0.0, i + 1) > tick).unwrap()
}

/// Shrinking an earlier pattern keeps the playhead in its arrangement slot,
/// at the same step within it, rather than at the same step overall.
#[test]
fn shrinking_an_earlier_pattern_keeps_the_slot_and_step() {
    let mut e = start(&arranged(16, "\"A\", \"B\", \"B\""), true);
    run(&mut e, 0, 200); // tick ~833: slot 2 (steps 16..31), step 10
    let old = e.score_view().unwrap().tick;
    let i = step_of(old);
    assert!((16..32).contains(&i), "slot 2, step {i}");

    push(&mut e, &arranged(8, "\"A\", \"B\", \"B\""), true).unwrap();
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
    let mut e = start(&arranged(16, "\"A\", \"B\", \"C\""), true);
    run(&mut e, 0, 264); // tick ~1100: slot 3 (steps 32..47)
    let old = e.score_view().unwrap().tick;
    assert!((32..48).contains(&step_of(old)));

    push(&mut e, &arranged(16, "\"B\", \"C\""), true).unwrap();
    let now = e.score_view().unwrap();
    assert_eq!((now.tick, now.length), (old - 500, 1000), "C, now slot 2");
}

/// A step past its slot's new length lands on that slot's end: the next
/// slot's first step.
#[test]
fn a_step_past_its_shrunk_slot_moves_to_the_next_slot() {
    let mut e = start(&arranged(16, "\"A\", \"B\""), true);
    run(&mut e, 0, 80); // tick ~333: slot 1, step 10
    assert!((8..16).contains(&step_of(e.score_view().unwrap().tick)));

    push(&mut e, &arranged(8, "\"A\", \"B\""), true).unwrap();
    assert_eq!(e.score_view().unwrap().tick, step_at(120.0, 0.0, 8));
}

/// Deleting the slot the playhead is in (not the last one) wraps or stops,
/// like any other slot that's gone: it doesn't fall back to whatever slot
/// now shares its index.
#[test]
fn deleting_the_playing_slot_wraps_or_stops() {
    for looping in [true, false] {
        let mut e = start(&arranged(16, "\"A\", \"C\", \"B\""), looping);
        run(&mut e, 0, 170); // tick ~708: slot 2 (C, steps 16..31)
        let old = e.score_view().unwrap().tick;
        assert!((16..32).contains(&step_of(old)));

        push(&mut e, &arranged(16, "\"A\", \"B\""), looping).unwrap();
        match e.score_view() {
            Some(v) if looping => assert_eq!((v.tick, v.length), (0, 1000), "wrapped"),
            None if !looping => {}
            other => panic!("loop = {looping}: {other:?}"),
        }
        let after = run(&mut e, 170, 171);
        assert_eq!(after["playing"], looping);
    }
}

/// Adding or removing a slot after the playing one is neither an insert nor
/// a delete at the playhead: the prefix up to and including the playing
/// slot still agrees, so it keeps playing that same slot, not wrap or stop
/// as if it had vanished.
#[test]
fn adding_or_removing_a_later_slot_keeps_the_playing_one() {
    let mut e = start(&arranged(16, "\"A\", \"B\", \"C\""), true);
    run(&mut e, 0, 80); // tick ~333: slot 1 (A, steps 0..15)
    let old = e.score_view().unwrap().tick;
    assert!((0..16).contains(&step_of(old)));

    push(&mut e, &arranged(16, "\"A\", \"B\", \"C\", \"B\""), true).unwrap();
    let now = e.score_view().unwrap();
    assert_eq!(
        (now.tick, now.length),
        (old, 2000),
        "appended slot, same slot playing"
    );

    push(&mut e, &arranged(16, "\"A\", \"B\""), true).unwrap();
    let now = e.score_view().unwrap();
    assert_eq!(
        (now.tick, now.length),
        (old, 1000),
        "trailing slots dropped, same slot playing"
    );
}

/// Plays `from` (16-step slots, 500 ticks each) for `frames` frames, then
/// pushes `to`: the tick before, and the tick and length after.
fn moved(from: &str, frames: u32, to: &str) -> (i64, i64, i64) {
    let mut e = start(&arranged(16, from), true);
    run(&mut e, 0, frames);
    let old = e.score_view().unwrap().tick;
    push(&mut e, &arranged(16, to), true).unwrap();
    let now = e.score_view().unwrap();
    (old, now.tick, now.length)
}

/// Deleting a pattern strips every slot it fills, on both sides of the
/// playhead in one write: the slots left are the old ones in order, so the
/// playing slot keeps playing at its new position.
#[test]
fn removing_slots_on_both_sides_keeps_the_playing_one() {
    // tick ~1100: slot 3 (C, steps 32..47)
    let (old, now, len) = moved("\"A\", \"B\", \"C\", \"B\"", 264, "\"A\", \"C\"");
    assert!((32..48).contains(&step_of(old)));
    assert_eq!((now, len), (old - 500, 1000), "C, now slot 2");

    // The second A plays; B goes from both sides.
    let (old, now, len) = moved("\"A\", \"B\", \"A\", \"B\"", 264, "\"A\", \"A\"");
    assert_eq!((now, len), (old - 500, 1000), "second A, now slot 2");
}

/// Slots inserted on both sides of the playhead in one write keep the
/// playing slot too.
#[test]
fn inserting_slots_on_both_sides_keeps_the_playing_one() {
    let (old, now, len) = moved(
        "\"A\", \"B\", \"C\"",
        264,
        "\"A\", \"B\", \"B\", \"C\", \"A\"",
    );
    assert!((32..48).contains(&step_of(old)));
    assert_eq!((now, len), (old + 500, 2500), "C, now slot 4");
}

/// Moving the playing slot one step earlier or later follows it: the
/// pattern keeps playing, not the neighbour that took its index.
#[test]
fn moving_the_playing_slot_follows_it() {
    // tick ~1100: slot 3 (C) moves earlier.
    let (old, now, len) = moved("\"A\", \"B\", \"C\"", 264, "\"A\", \"C\", \"B\"");
    assert_eq!((now, len), (old - 500, 1500), "C, now slot 2");

    // tick ~708: slot 2 (C) moves later.
    let (old, now, len) = moved("\"A\", \"C\", \"B\"", 170, "\"A\", \"B\", \"C\"");
    assert!((16..32).contains(&step_of(old)));
    assert_eq!((now, len), (old + 500, 1500), "C, now slot 3");
}
