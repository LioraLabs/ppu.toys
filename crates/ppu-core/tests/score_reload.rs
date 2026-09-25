//! Hot reload of a playing song: a push that changes only a song file
//! (`seq_<id>` and nothing else) re-runs it in the live VM, and every
//! `score{ song = "<id>" }` recompiles in place, keeping its tick. Driven
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
        run(&mut e, 0, 90); // tick ~375
        push(&mut e, &beat("\"4.......\""), looping).unwrap(); // length 250
                                                               // Two frames: frame() runs before the timers, so the second sees the stop.
        let after = run(&mut e, 90, 92);
        assert_eq!(after["frames"], 92, "in place");
        match e.score_view() {
            Some(v) if looping => {
                assert_eq!(v.length, 250);
                assert!(v.tick < 10, "wrapped to {}", v.tick);
            }
            None if !looping => assert_eq!(after["playing"], false),
            other => panic!("loop = {looping}: {other:?}"),
        }
    }
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
