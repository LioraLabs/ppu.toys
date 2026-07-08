//! E7 sanity: drive the real LuaEngine -> compositor pipeline end-to-end the way
//! the wasm shim does, without wasm-bindgen.
use ppu_core::{derive_registers, render_frame, LuaEngine, OamSprite, WIDTH};
use std::collections::HashMap;

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
    let err = engine
        .set_source("function frame(t,f) this is not lua end")
        .unwrap_err();
    assert!(!err.message.is_empty());
}

#[test]
fn upload_asset_lists_and_dedups_generation() {
    let mut e = LuaEngine::new();
    e.upload_asset("sky".into(), 2, 1, vec![255, 0, 0, 255, 0, 0, 255, 255]);
    e.upload_asset("hills".into(), 1, 1, vec![0, 255, 0, 255]);
    let a = e.assets();
    assert_eq!(a.len(), 2);
    assert_eq!(a[0], ("hills".into(), 1, 1)); // sorted by id
    e.upload_asset("bad".into(), 4, 4, vec![0; 3]); // malformed -> ignored
    assert_eq!(e.assets().len(), 2);
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

#[test]
fn lua_tm_ts_round_trip_through_register_state() {
    let mut engine = LuaEngine::new();
    // Frame-wide default set, plus a hook that repaints TM on a band — exercises
    // both write_state (baseline) and read_state (readback).
    let src = r#"
        function frame(t, f)
            TM = 0x13   -- BG1+BG2+OBJ on the main screen
            TS = 0x04   -- BG3 on the sub screen
            hdma(100, 120, function(y) TM = 0x1f end)
        end
    "#;
    engine.set_source(src).expect("source compiles");
    let lt = engine.frame(0.0, 0).expect("frame runs");
    // Frame-wide default row.
    assert_eq!(lt.rows[0].tm, 0x13);
    assert_eq!(lt.rows[0].ts, 0x04);
    // Inside the hook band TM is repainted; TS stays the sticky default.
    assert_eq!(lt.rows[110].tm, 0x1f);
    assert_eq!(lt.rows[110].ts, 0x04);
}

#[test]
fn lua_window_registers_round_trip_and_animate() {
    let mut engine = LuaEngine::new();
    let src = r#"
        function frame(t, f)
            WH0 = 0
            WH1 = 40
            WH2 = 5
            WH3 = 250
            W12SEL = 0x02   -- BG1 window 1 enable
            W34SEL = 0x00
            WOBJSEL = 0x00
            WBGLOG = 0x00
            WOBJLOG = 0x00
            TMW = 0x01      -- clip BG1 inside the window on the main screen
            TSW = 0x00
            -- iris sweep: window 1 right edge widens down the frame.
            hdma(0, 223, function(y) WH1 = y end)
        end
    "#;
    engine.set_source(src).expect("source compiles");
    let lt = engine.frame(0.0, 0).expect("frame runs");
    // Frame-wide defaults captured on row 0's baseline registers.
    assert_eq!(lt.rows[0].wh0, 0);
    assert_eq!(lt.rows[0].w12sel, 0x02);
    assert_eq!(lt.rows[0].tmw, 0x01);
    // The hdma hook animates WH1 == y down the frame.
    assert_eq!(lt.rows[0].wh1, 0);
    assert_eq!(lt.rows[50].wh1, 50);
    assert_eq!(lt.rows[200].wh1, 200);
}
