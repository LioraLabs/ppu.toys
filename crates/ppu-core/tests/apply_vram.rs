//! `apply_vram()` in ppuglobals.lua runs implicitly every frame, before
//! `apply_pokes()`, and its `vr()` writes are undo-tracked like raw pokes.
use ppu_core::LuaEngine;

const MAIN: &str = "function frame(t, f)\n  vram[0x4001] = 0x0009\nend\n";

fn controls(vram_body: &str) -> String {
    format!(
        "markers = {{\n}}\n\nscanlines = {{\n}}\n\n{vram_body}function apply_pokes()\n  brightness = 7\nend\n"
    )
}

#[test]
fn apply_vram_runs_implicitly_and_wins_over_the_program() {
    let mut e = LuaEngine::new();
    let tiles = "function apply_vram()\n  vr(0x4000, { 0x0001, 0x0002, 0x0003 })\nend\n\n";
    e.set_sources(&[("ppuglobals.lua", controls(tiles).as_str()), ("main.lua", MAIN)])
        .unwrap();
    let lt = e.frame(0.0, 0).unwrap();
    assert_eq!(&e.memory().vram[0x4000..0x4003], &[1, 2, 3], "pokes always win");
    assert_eq!(lt.rows[0].brightness, 7, "apply_pokes still runs");
}

#[test]
fn releasing_the_tiles_gives_the_words_back_to_the_program() {
    let mut e = LuaEngine::new();
    let tiles = "function apply_vram()\n  vr(0x4000, { 0x0001, 0x0002, 0x0003 })\nend\n\n";
    e.set_sources(&[("ppuglobals.lua", controls(tiles).as_str()), ("main.lua", MAIN)])
        .unwrap();
    e.frame(0.0, 0).unwrap();
    // controls-only edit (the in-place reload path): tiles removed
    e.set_sources(&[("ppuglobals.lua", controls("").as_str()), ("main.lua", MAIN)])
        .unwrap();
    e.frame(0.0, 1).unwrap();
    assert_eq!(e.memory().vram[0x4001], 9, "the program's own write shows again");
    assert_eq!(e.memory().vram[0x4000], 0, "a poked-only word is released, not baked in");
}

#[test]
fn vr_is_a_plain_persistent_write_in_a_user_program() {
    let mut e = LuaEngine::new();
    let main = "function init()\n  vr(0x100, { 5, 6 })\nend\nfunction frame(t, f) end\n";
    e.set_sources(&[("main.lua", main)]).unwrap();
    e.frame(0.0, 0).unwrap();
    e.frame(0.0, 1).unwrap();
    assert_eq!(&e.memory().vram[0x100..0x102], &[5, 6]);
}

#[test]
fn an_error_inside_apply_vram_is_attributed_to_ppuglobals() {
    let mut e = LuaEngine::new();
    let bad = "function apply_vram()\n  vr(0x4000, nope.words)\nend\n\n";
    let err = e
        .set_sources(&[("ppuglobals.lua", controls(bad).as_str()), ("main.lua", MAIN)])
        .unwrap_err();
    assert_eq!(err.file.as_deref(), Some("ppuglobals.lua"));
}

#[test]
fn tiles_still_inside_a_legacy_apply_pokes_keep_working() {
    let mut e = LuaEngine::new();
    let legacy = "markers = {\n}\n\nscanlines = {\n}\n\nfunction apply_pokes()\n  vram[0x4001] = 0x0042\nend\n";
    e.set_sources(&[("ppuglobals.lua", legacy), ("main.lua", MAIN)]).unwrap();
    e.frame(0.0, 0).unwrap();
    assert_eq!(e.memory().vram[0x4001], 0x42);
}
