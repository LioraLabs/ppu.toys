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

#[test]
fn an_added_song_file_recompiles() {
    let mut e = start(&beat("\"4...............\""), true);
    run(&mut e, 0, 30);
    let init = "function init() h = score{ song = \"beat\" } end";
    e.set_sources(&[
        ("beat.lua", &beat("\"4...............\"")),
        ("other.lua", "function seq_other() return {} end\n"),
        ("main.lua", &format!("{MAIN}\n{init}")),
    ])
    .unwrap();
    assert_eq!(run(&mut e, 30, 31)["frames"], 1, "recompiled");
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
