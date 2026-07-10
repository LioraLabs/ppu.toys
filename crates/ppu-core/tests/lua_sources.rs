//! Multi-chunk `set_sources` semantics: PICO-8 shared scope, list-order
//! execution, per-file error attribution, fresh globals on recompile.
use ppu_core::LuaEngine;

#[test]
fn chunks_execute_in_list_order_into_shared_globals() {
    let mut e = LuaEngine::new();
    e.set_sources(&[
        ("a.lua", "x = 1"),
        ("b.lua", "x = x + 1"),
        ("main.lua", "function frame(t,f) brightness = x end"),
    ])
    .unwrap();
    let lt = e.frame(0.0, 0).unwrap();
    assert_eq!(lt.rows[0].brightness, 2);
}

#[test]
fn function_defined_in_one_file_is_callable_from_another() {
    let mut e = LuaEngine::new();
    e.set_sources(&[
        ("util.lua", "function tint() return 5 end"),
        ("main.lua", "function frame(t,f) brightness = tint() end"),
    ])
    .unwrap();
    assert_eq!(e.frame(0.0, 0).unwrap().rows[0].brightness, 5);
}

#[test]
fn frame_resolves_after_all_chunks_and_main_lua_is_not_special() {
    // frame() lives in the FIRST file and calls a helper defined in a LATER
    // file; no file is named main.lua. Works because frame is resolved (and
    // first called) only after every chunk has executed.
    let mut e = LuaEngine::new();
    e.set_sources(&[
        ("scene.lua", "function frame(t,f) brightness = glow() end"),
        ("fx.lua", "function glow() return 7 end"),
    ])
    .unwrap();
    assert_eq!(e.frame(0.0, 0).unwrap().rows[0].brightness, 7);
}

#[test]
fn recompile_starts_from_fresh_globals() {
    let mut e = LuaEngine::new();
    e.set_sources(&[(
        "main.lua",
        "leak = 9\nfunction frame(t,f) brightness = leak end",
    )])
    .unwrap();
    assert_eq!(e.frame(0.0, 0).unwrap().rows[0].brightness, 9);
    // `leak` must NOT survive the recompile: fresh VM, fresh globals.
    e.set_sources(&[("main.lua", "function frame(t,f) brightness = leak or 3 end")])
        .unwrap();
    assert_eq!(e.frame(0.0, 0).unwrap().rows[0].brightness, 3);
}

#[test]
fn compile_error_attributes_file_and_per_file_line() {
    let mut e = LuaEngine::new();
    let err = e
        .set_sources(&[("ok.lua", "x = 1"), ("bad.lua", "y = 2\nz = = 3")])
        .unwrap_err();
    assert_eq!(err.file.as_deref(), Some("bad.lua"));
    assert_eq!(err.line, Some(2)); // line within bad.lua, not a concatenation
    assert!(!err.message.is_empty());
}

#[test]
fn top_level_runtime_error_attributes_the_failing_chunk() {
    let mut e = LuaEngine::new();
    let err = e
        .set_sources(&[
            ("a.lua", "x = 1"),
            ("b.lua", "x = x + not_defined.field"), // indexes nil as the chunk executes
        ])
        .unwrap_err();
    assert_eq!(err.file.as_deref(), Some("b.lua"));
}

#[test]
fn failed_recompile_keeps_the_previous_program_running() {
    let mut e = LuaEngine::new();
    e.set_sources(&[("main.lua", "function frame(t,f) brightness = 5 end")])
        .unwrap();
    assert_eq!(e.frame(0.0, 0).unwrap().rows[0].brightness, 5);
    // Chunk error in the new sketch: the swap happens only after every chunk
    // succeeds, so the previously loaded program must keep running untouched.
    e.set_sources(&[("main.lua", "function frame(t,f)\n  = broken\nend")])
        .unwrap_err();
    assert_eq!(e.frame(0.0, 0).unwrap().rows[0].brightness, 5);
}

#[test]
fn set_source_sugar_attributes_the_sole_file() {
    let mut e = LuaEngine::new();
    let err = e
        .set_source("function frame(t,f)\n  mode = = 7\nend")
        .unwrap_err();
    assert_eq!(err.file.as_deref(), Some("source"));
    assert_eq!(err.line, Some(2));
}

#[test]
fn frame_runtime_error_attributes_frames_defining_file() {
    let mut e = LuaEngine::new();
    e.set_sources(&[
        ("util.lua", "function safe() return 1 end"),
        ("game.lua", "function frame(t,f) error('boom') end"),
    ])
    .unwrap();
    let err = e.frame(0.0, 0).unwrap_err();
    assert_eq!(err.file.as_deref(), Some("game.lua"));
    assert!(err.message.contains("boom"));
}

#[test]
fn hook_runtime_error_attributes_the_hooks_defining_file() {
    let mut e = LuaEngine::new();
    e.set_sources(&[
        ("fx.lua", "function fx_hook(y) error('kaboom') end"),
        ("main.lua", "function frame(t,f) hdma(0, 10, fx_hook) end"),
    ])
    .unwrap();
    let err = e.frame(0.0, 0).unwrap_err();
    assert_eq!(err.file.as_deref(), Some("fx.lua"));
    assert!(err.message.contains("kaboom"));
}

#[test]
fn init_runtime_error_attributes_inits_defining_file() {
    let mut e = LuaEngine::new();
    let err = e
        .set_sources(&[
            ("setup.lua", "function init() error('bad init') end"),
            ("main.lua", "function frame(t,f) end"),
        ])
        .unwrap_err();
    assert_eq!(err.file.as_deref(), Some("setup.lua"));
}
