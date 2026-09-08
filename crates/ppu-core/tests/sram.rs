use ppu_core::LuaEngine;

const PROG: &str = "function init()\n  sram.best = sram.best or 0\n  sram.log = sram.log or {}\nend\nfunction frame(t, f)\n  if pad.a then sram.best = sram.best + 1; sram.log[#sram.log + 1] = 'hit' end\n  brightness = sram.best\nend\n";

#[test]
fn sram_round_trips_through_json_and_recompiles() {
    let mut e = LuaEngine::new();
    e.set_source(PROG).unwrap();
    // init() wrote defaults: dirty after the first frame
    assert_eq!(e.frame(0.0, 0).unwrap().rows[0].brightness, 0);
    assert_eq!(e.take_sram().as_deref(), Some(r#"{"best":0,"log":{}}"#));
    assert_eq!(e.take_sram(), None); // taken

    e.set_pad(1 << 4); // a
    e.frame(0.0, 1).unwrap();
    e.set_pad(0);
    let saved = e.take_sram().unwrap();
    assert_eq!(saved, r#"{"best":1,"log":["hit"]}"#);
    assert_eq!(e.frame(0.0, 2).unwrap().rows[0].brightness, 1);
    assert_eq!(e.take_sram(), None); // unchanged frame: not dirty

    // A fresh engine (page reload) seeded with the blob: init() sees it.
    let mut e2 = LuaEngine::new();
    e2.set_sram(&saved);
    e2.set_source(PROG).unwrap();
    assert_eq!(e2.frame(0.0, 0).unwrap().rows[0].brightness, 1);
    assert_eq!(e2.take_sram(), None); // host-provided blob is not dirty

    // Recompile keeps the data; garbage resets to empty.
    e2.set_source(PROG).unwrap();
    assert_eq!(e2.frame(0.0, 1).unwrap().rows[0].brightness, 1);
    e2.set_sram("not json");
    assert_eq!(e2.frame(0.0, 2).unwrap().rows[0].brightness, 15); // live table blanked: sram.best is nil...
    e2.set_source(PROG).unwrap();
    assert_eq!(e2.frame(0.0, 3).unwrap().rows[0].brightness, 0); // ...until recompile

    // Unserializable content is a runtime error naming the path.
    let mut e3 = LuaEngine::new();
    e3.set_source("function frame() sram.cb = function() end end")
        .unwrap();
    let err = e3.frame(0.0, 0).unwrap_err();
    assert!(err.message.contains("sram.cb"), "{}", err.message);
    let mut e4 = LuaEngine::new();
    e4.set_source("function frame() for i = 1, 5000 do sram[i] = 'padding' end end")
        .unwrap();
    let big = e4.frame(0.0, 0).unwrap_err();
    assert!(big.message.contains("bytes"), "{}", big.message);
}
