//! Live pokes: `set_pokes` hands the engine a chunk that is no file of the program: optional
//! top-level `dma()` placements, then `__ppu_pokes`, applied through a restore-every-frame pass.
//! It survives every load, a chunk that fails to load is refused, a placement change recompiles,
//! and its errors carry the label "pokes".
mod common;
use ppu_core::LuaEngine;

/// The chunk shape the Studio generates: scanlines, then __ppu_pokes.
fn chunk(body: &str) -> String {
    format!("scanlines = {{\n}}\n\nfunction __ppu_pokes()\n{body}end\n")
}

fn placed(dma: &str, body: &str) -> String {
    format!("{dma}\n{}", chunk(body))
}

const MAIN: &str = "function init() n = 0 end\n\
                    function frame(t, f) n = n + 1; brightness = 5; bg[2].scroll.x = n end\n";

fn sky(e: &mut LuaEngine) {
    common::add_bg(e, "sky", [0u8, 255, 0, 255].repeat(256 * 224), 256, 224, 4);
}

#[test]
fn a_chunk_applies_and_empty_releases_it() {
    let mut e = LuaEngine::new();
    e.set_sources(&[("main.lua", MAIN)]).unwrap();
    assert_eq!(e.frame(0.0, 0).unwrap().rows[0].brightness, 5);
    e.set_pokes(&chunk("  brightness = 12\n")).unwrap();
    let lt = e.frame(0.0, 1).unwrap();
    assert_eq!(lt.rows[0].brightness, 12);
    assert_eq!(
        lt.rows[0].bg[1].scroll_x, 2,
        "setting pokes never reruns init"
    );
    e.set_pokes("").unwrap();
    assert_eq!(e.frame(0.0, 2).unwrap().rows[0].brightness, 5);
}

#[test]
fn a_chunk_survives_a_recompile_and_a_reset() {
    let mut e = LuaEngine::new();
    e.set_sources(&[("main.lua", MAIN)]).unwrap();
    e.set_pokes(&chunk("  brightness = 12\n")).unwrap();
    assert_eq!(e.frame(0.0, 0).unwrap().rows[0].brightness, 12);

    let edited = format!("{MAIN}-- edited\n");
    e.set_sources(&[("main.lua", &edited)]).unwrap();
    assert_eq!(e.frame(0.0, 1).unwrap().rows[0].brightness, 12);

    e.reset().unwrap();
    assert_eq!(e.frame(0.0, 0).unwrap().rows[0].brightness, 12);
}

#[test]
fn a_rejected_chunk_keeps_the_previous_one_and_the_next_load_succeeds() {
    let mut e = LuaEngine::new();
    e.set_sources(&[("main.lua", MAIN)]).unwrap();
    e.set_pokes(&chunk("  brightness = 12\n")).unwrap();
    let err = e.set_pokes("function __ppu_pokes(\n").unwrap_err();
    assert_eq!(err.file.as_deref(), Some("pokes"));
    assert_eq!(e.frame(0.0, 0).unwrap().rows[0].brightness, 12);
    let edited = format!("{MAIN}-- edited\n");
    e.set_sources(&[("main.lua", &edited)])
        .expect("a refused chunk never breaks a load");
    assert_eq!(e.frame(0.0, 1).unwrap().rows[0].brightness, 12);
}

#[test]
fn a_runtime_error_in_the_pokes_carries_the_pokes_label() {
    let mut e = LuaEngine::new();
    e.set_sources(&[("main.lua", MAIN)]).unwrap();
    e.set_pokes(&chunk("  error('boom')\n")).unwrap();
    assert_eq!(e.frame(0.0, 0).unwrap_err().file.as_deref(), Some("pokes"));
}

#[test]
fn a_dma_line_in_the_chunk_places_the_source_on_load() {
    let mut e = LuaEngine::new();
    sky(&mut e);
    e.set_pokes(&placed(
        "dma(\"sky\", { char = 0x1000, map = 0x0, pal = 0 })",
        "  brightness = 7\n",
    ))
    .unwrap(); // before any program: kept for the first load
    e.set_sources(&[("main.lua", MAIN)]).unwrap();
    assert_eq!(e.frame(0.0, 0).unwrap().rows[0].brightness, 7);
    let p = &e.memory_map().placements;
    assert_eq!((p.len(), p[0].name.as_str(), p[0].char), (1, "sky", 0x1000));
}

#[test]
fn a_placement_change_recompiles_and_a_pokes_change_does_not() {
    let mut e = LuaEngine::new();
    sky(&mut e);
    e.set_sources(&[("main.lua", MAIN)]).unwrap();
    let at = |char: &str| format!("dma(\"sky\", {{ char = {char}, map = 0x0, pal = 0 }})");
    e.set_pokes(&placed(&at("0x1000"), "")).unwrap();
    e.frame(0.0, 0).unwrap(); // recompiled: init ran, n = 1
    assert_eq!(e.memory_map().placements[0].char, 0x1000);

    // pokes-only change: in place, the VM (and n) kept
    e.set_pokes(&placed(&at("0x1000"), "  brightness = 9\n"))
        .unwrap();
    let lt = e.frame(0.0, 1).unwrap();
    assert_eq!(
        (lt.rows[0].brightness, lt.rows[0].bg[1].scroll_x),
        (9, 2),
        "no recompile"
    );

    // placement change: the next frame recompiles (init reran, n back to 1)
    e.set_pokes(&placed(&at("0x2000"), "  brightness = 9\n"))
        .unwrap();
    let lt = e.frame(0.0, 2).unwrap();
    assert_eq!(
        (lt.rows[0].brightness, lt.rows[0].bg[1].scroll_x),
        (9, 1),
        "recompiled"
    );
    assert_eq!(e.memory_map().placements[0].char, 0x2000);

    // dropping the chunk drops its placement too
    e.set_pokes("").unwrap();
    e.frame(0.0, 3).unwrap();
    assert!(e.memory_map().placements.is_empty());
}

#[test]
fn a_dma_line_naming_a_missing_source_errors_as_pokes() {
    let mut e = LuaEngine::new();
    e.set_sources(&[("main.lua", MAIN)]).unwrap();
    e.set_pokes(&placed(
        "dma(\"nope\", { char = 0x1000, map = 0x0, pal = 0 })",
        "",
    ))
    .unwrap();
    assert_eq!(e.frame(0.0, 0).unwrap_err().file.as_deref(), Some("pokes"));

    let mut fresh = LuaEngine::new();
    fresh.set_pokes(&placed("dma(\"nope\")", "")).unwrap();
    assert_eq!(
        fresh
            .set_sources(&[("main.lua", MAIN)])
            .unwrap_err()
            .file
            .as_deref(),
        Some("pokes")
    );
}

#[test]
fn a_file_named_ppuglobals_lua_is_ordinary_code_and_its_apply_pokes_is_never_applied() {
    let mut e = LuaEngine::new();
    let file = "function apply_pokes() brightness = 3 end\n";
    e.set_sources(&[("main.lua", MAIN), ("ppuglobals.lua", file)])
        .unwrap();
    assert_eq!(e.frame(0.0, 0).unwrap().rows[0].brightness, 5);
}

#[test]
fn the_chunks_painted_tiles_overlay_vram_and_empty_drops_them() {
    let mut e = LuaEngine::new();
    e.set_sources(&[("main.lua", "function frame(t, f) end")])
        .unwrap();
    e.set_pokes("function __ppu_pokes_vram()\n  vr(0x0100, { 0x1234 })\nend\n")
        .unwrap();
    e.frame(0.0, 0).unwrap();
    assert_eq!(e.memory().vram[0x0100], 0x1234);
    e.set_pokes("").unwrap();
    e.frame(0.0, 1).unwrap();
    assert_eq!(e.memory().vram[0x0100], 0);
}
