//! Paused, the Studio draws the same frame again for every poke: that repeat starts from the
//! state the frame first started from, so a stateful program does not creep forward (PPU-834).
use ppu_core::LuaEngine;

const SRC: &str = "local x = 0\nlocal seen = {}\nfunction frame(t, f) x = x + 1 seen[#seen + 1] = f end\nfunction probe() return {x, #seen} end";

fn probe(e: &mut LuaEngine) -> Vec<i64> {
    e.call_ints("probe", &[]).unwrap()
}

#[test]
fn a_paused_repeat_does_not_advance_the_program() {
    let mut e = LuaEngine::new();
    e.set_sources(&[("main.luau", SRC)]).unwrap();
    for f in 0..10 {
        e.frame(f as f64 / 60.0, f).unwrap();
    }
    e.set_hold(true);
    e.frame(10.0 / 60.0, 10).unwrap(); // the frame paused on
    assert_eq!(probe(&mut e), vec![11, 11]);
    for _ in 0..5 {
        e.frame(10.0 / 60.0, 10).unwrap(); // a poke redraws it
    }
    assert_eq!(probe(&mut e), vec![11, 11]);
    e.frame(11.0 / 60.0, 11).unwrap(); // a step forward advances once
    assert_eq!(probe(&mut e), vec![12, 12]);
    e.set_hold(false);
    e.frame(12.0 / 60.0, 12).unwrap();
    assert_eq!(probe(&mut e), vec![13, 13]);
}

#[test]
fn playing_repeats_still_run_as_before() {
    let mut e = LuaEngine::new();
    e.set_sources(&[("main.luau", SRC)]).unwrap();
    e.frame(0.0, 0).unwrap();
    e.frame(0.0, 0).unwrap();
    assert_eq!(probe(&mut e), vec![2, 2]);
}

#[test]
fn a_poke_shows_on_the_paused_frame_and_nothing_else_moves() {
    let mut e = LuaEngine::new();
    e.set_sources(&[("main.luau", SRC)]).unwrap();
    e.set_hold(true);
    e.frame(0.0, 0).unwrap();
    e.set_pokes("function __ppu_pokes()\n  cgram[0] = 0x1234\nend\n")
        .unwrap();
    e.frame(0.0, 0).unwrap();
    assert_eq!(e.memory().cgram[0], 0x1234);
    assert_eq!(probe(&mut e), vec![1, 1]);
}
