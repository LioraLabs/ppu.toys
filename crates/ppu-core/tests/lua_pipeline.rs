//! E7 sanity: drive the real LuaEngine -> compositor pipeline end-to-end the way
//! the wasm shim does, without wasm-bindgen.
use std::collections::HashMap;
use ppu_core::{derive_registers, render_frame, LuaEngine, OamSprite, WIDTH};

#[test]
fn lua_source_drives_backdrop_through_compositor() {
    let mut engine = LuaEngine::new();
    let src = r#"
        function frame(t, f)
            brightness = 15
            mode = 1
            cgram[0] = rgb(255, 0, 0)
        end
    "#;
    engine.set_source(src).expect("source compiles");
    let lt = engine.frame(0.0, 0).expect("frame runs");
    let fb = render_frame(&lt, engine.memory());
    assert_eq!(fb.len(), WIDTH * 224 * 4);
    // cgram[0] = rgb(255,0,0) -> backdrop red (5-bit expanded: expand(31) = 255).
    assert_eq!(&fb[0..4], &[255, 0, 0, 255]);
    // the resolved absolute row carries the frame's mode — assert both the
    // source-of-truth and the inspector derivation agree.
    assert_eq!(lt.rows[0].mode, 1);
    let regs = derive_registers(&lt.rows[0], &HashMap::new());
    let bgmode = regs.iter().find(|r| r.name == "BGMODE").unwrap();
    assert_eq!(bgmode.value, 1);
}

#[test]
fn lua_oam_maps_to_sprite_views() {
    let mut engine = LuaEngine::new();
    let src = r#"
        function frame(t, f)
            obj[0].on = true
            obj[0].x = 10
            obj[0].tile = 3
            obj[0].flip_x = true
        end
    "#;
    engine.set_source(src).expect("source compiles");
    let _ = engine.frame(0.0, 0).expect("frame runs");
    let sprites: Vec<OamSprite> = engine.memory().oam.iter().map(OamSprite::from).collect();
    assert_eq!(sprites.len(), 128);
    assert!(sprites[0].on);
    assert_eq!(sprites[0].x, 10);
    assert_eq!(sprites[0].tile, 3);
    assert!(sprites[0].flip_x);
}

#[test]
fn setsource_reports_compile_error() {
    let mut engine = LuaEngine::new();
    let err = engine.set_source("function frame(t,f) this is not lua end").unwrap_err();
    assert!(!err.message.is_empty());
}

#[test]
fn frame_reports_runtime_error() {
    let mut engine = LuaEngine::new();
    // index a nil global at frame() time -> Lua runtime error
    engine
        .set_source("function frame(t, f) local x = nope.field end")
        .expect("compiles");
    let result = engine.frame(0.0, 0);
    assert!(result.is_err(), "expected a runtime error from frame()");
    let err = result.err().unwrap();
    assert!(!err.message.is_empty(), "runtime error carries a message");
}
