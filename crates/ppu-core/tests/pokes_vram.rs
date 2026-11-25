//! The pokes chunk's `__ppu_pokes_vram()` runs implicitly every frame, before
//! `__ppu_pokes()`, and its `vr()` writes are undo-tracked like raw pokes.
use ppu_core::LuaEngine;

const MAIN: &str = "function frame(t, f)\n  vram[0x4001] = 0x0009\nend\n";

fn pokes(vram_body: &str) -> String {
    format!("scanlines = {{\n}}\n\n{vram_body}function __ppu_pokes()\n  brightness = 7\nend\n")
}

const TILES: &str =
    "function __ppu_pokes_vram()\n  vr(0x4000, { 0x0001, 0x0002, 0x0003 })\nend\n\n";

#[test]
fn painted_tiles_run_implicitly_and_win_over_the_program() {
    let mut e = LuaEngine::new();
    e.set_sources(&[("main.lua", MAIN)]).unwrap();
    e.set_pokes(&pokes(TILES)).unwrap();
    let lt = e.frame(0.0, 0).unwrap();
    assert_eq!(
        &e.memory().vram[0x4000..0x4003],
        &[1, 2, 3],
        "pokes always win"
    );
    assert_eq!(lt.rows[0].brightness, 7, "__ppu_pokes still runs");
}

#[test]
fn releasing_the_tiles_gives_the_words_back_to_the_program() {
    let mut e = LuaEngine::new();
    e.set_sources(&[("main.lua", MAIN)]).unwrap();
    e.set_pokes(&pokes(TILES)).unwrap();
    e.frame(0.0, 0).unwrap();
    // in-place reload: tiles removed
    e.set_pokes(&pokes("")).unwrap();
    e.frame(0.0, 1).unwrap();
    assert_eq!(
        e.memory().vram[0x4001],
        9,
        "the program's own write shows again"
    );
    assert_eq!(
        e.memory().vram[0x4000],
        0,
        "a poked-only word is released, not baked in"
    );
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
fn an_error_inside_the_painted_tiles_carries_the_pokes_label() {
    let mut e = LuaEngine::new();
    let bad = "function __ppu_pokes_vram()\n  vr(0x4000, nope.words)\nend\n\n";
    e.set_pokes(&pokes(bad)).unwrap();
    let err = e.set_sources(&[("main.lua", MAIN)]).unwrap_err();
    assert_eq!(err.file.as_deref(), Some("pokes"));
}

#[test]
fn raw_vram_pokes_inside_ppu_pokes_keep_working() {
    let mut e = LuaEngine::new();
    e.set_sources(&[("main.lua", MAIN)]).unwrap();
    e.set_pokes("function __ppu_pokes()\n  vram[0x4001] = 0x0042\nend\n")
        .unwrap();
    e.frame(0.0, 0).unwrap();
    assert_eq!(e.memory().vram[0x4001], 0x42);
}
