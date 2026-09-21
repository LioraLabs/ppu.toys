use ppu_core::{render_frame_view, LuaEngine};
use std::time::Instant;

const HOOK: &str = "  hdma(0, 223, function(y)
    local m7_view1 = mode7_floor(y, 80, 43, 0, 20, 605)
    bg[1].scroll.x = m7_view1.scroll_x
    bg[1].scroll.y = m7_view1.scroll_y
    m7.a = m7_view1.a
    m7.b = m7_view1.b
    m7.c = m7_view1.c
    m7.cx = m7_view1.cx
    m7.cy = m7_view1.cy
    m7.d = m7_view1.d
    screen.main.bg1 = m7_view1.visible
  end)
";

fn ms(files: &[(&str, &str)]) -> f64 {
    let mut e = LuaEngine::new();
    e.set_sources(files).unwrap();
    for f in 0..5 { e.frame(0.0, f).unwrap(); }
    let t = Instant::now();
    for f in 0..60 { e.frame(0.0, f).unwrap(); }
    t.elapsed().as_secs_f64() * 1000.0 / 60.0
}

#[test]
fn print_costs() {
    let head = "markers = {\n}\n\nscanlines = {\n}\n\n";
    let controls = format!("{head}function apply_pokes()\n{HOOK}end\n");
    let empty = format!("{head}function apply_pokes()\nend\n");
    let main_hook = format!("function frame(t, f)\n  mode = 7\n{HOOK}end\n");
    let main_plain = "function frame(t, f)\n  mode = 7\nend\n";
    let only_call = format!("{head}function apply_pokes()\n  hdma(0, 223, function(y)\n    local v = mode7_floor(y, 80, 43, 0, 20, 605)\n  end)\nend\n");
    println!("baseline (no hook):              {:.2} ms/frame", ms(&[("ppuglobals.lua", &empty), ("main.lua", main_plain)]));
    println!("hook in main.lua (untracked):    {:.2} ms/frame", ms(&[("ppuglobals.lua", &empty), ("main.lua", &main_hook)]));
    println!("hook in apply_pokes (tracked):   {:.2} ms/frame", ms(&[("ppuglobals.lua", &controls), ("main.lua", main_plain)]));
    println!("apply_pokes: mode7_floor only:   {:.2} ms/frame", ms(&[("ppuglobals.lua", &only_call), ("main.lua", main_plain)]));
}

/// A generated Mode 7 floor view (one hook, 9 register writes, 224 rows) must
/// cost about what the same hook costs hand-written in main.lua. Before the
/// per-row fast path it was 2.6x (3.85 ms vs 1.48 ms native) and took the
/// Studio from 60 to 28 fps under WASM.
#[test]
fn a_generated_floor_view_costs_about_what_a_handwritten_one_does() {
    let head = "markers = {\n}\n\nscanlines = {\n}\n\n";
    let controls = format!("{head}function apply_pokes()\n{HOOK}end\n");
    let empty = format!("{head}function apply_pokes()\nend\n");
    let main_hook = format!("function frame(t, f)\n  mode = 7\n{HOOK}end\n");
    let hand = ms(&[("ppuglobals.lua", &empty), ("main.lua", &main_hook)]);
    let generated = ms(&[("ppuglobals.lua", &controls), ("main.lua", "function frame(t, f)\n  mode = 7\nend\n")]);
    println!("hand-written {hand:.2} ms, generated {generated:.2} ms");
    assert!(generated < hand * 2.0 + 1.0, "generated {generated:.2} ms vs hand-written {hand:.2} ms");
}

/// Lua phase vs render phase, as the WASM frame() does them.
#[test]
fn split_lua_and_render() {
    let head = "markers = {\n}\n\nscanlines = {\n}\n\n";
    let main = "function frame(t, f)\n  mode = 7\n  screen.main.bg1 = true\nend\n";
    for (label, body) in [("no view   ", String::new()), ("floor view", HOOK.to_string())] {
        let controls = format!("{head}function apply_pokes()\n{body}end\n");
        let mut e = LuaEngine::new();
        e.set_sources(&[("ppuglobals.lua", &controls), ("main.lua", main)]).unwrap();
        let (mut lua, mut render) = (0.0, 0.0);
        for f in 0..40 {
            let t = Instant::now();
            let lt = e.frame(0.0, f).unwrap();
            lua += t.elapsed().as_secs_f64();
            let t = Instant::now();
            let _ = render_frame_view(&lt, e.memory());
            render += t.elapsed().as_secs_f64();
        }
        println!("{label}: lua {:.2} ms, render {:.2} ms per frame", lua * 25.0, render * 25.0);
    }
}

/// The per-row fast path skips undo logging for register writes. The thing it
/// must not break: a band override never bakes into the program's own state.
#[test]
fn a_band_override_of_an_accumulating_register_never_bakes_in() {
    let main = "function frame(t, f)\n  bg[1].scroll.x = bg[1].scroll.x + 1\n  m7.a = m7.a + 0.5\nend\n";
    let band = "markers = {\n}\n\nscanlines = {\n  mid = { first = 10, last = 20 },\n}\n\nfunction apply_pokes()\n  hdma(scanlines.mid.first, scanlines.mid.last, function(y)\n    bg[1].scroll.x = 100\n    m7.a = 9\n  end)\nend\n";
    let none = "markers = {\n}\n\nscanlines = {\n}\n\nfunction apply_pokes()\nend\n";
    let mut e = LuaEngine::new();
    e.set_sources(&[("ppuglobals.lua", band), ("main.lua", main)]).unwrap();
    let mut lt = e.frame(0.0, 0).unwrap();
    for f in 1..3 {
        lt = e.frame(0.0, f).unwrap();
    }
    assert_eq!(lt.rows[0].bg[0].scroll_x, 3, "the program's accumulator advanced once per frame");
    assert_eq!(lt.rows[15].bg[0].scroll_x, 100, "the band overrides inside its range");
    assert_eq!(lt.rows[21].bg[0].scroll_x, 3, "and nowhere else");
    // release the band (controls-only reload): the program's value shows again
    e.set_sources(&[("ppuglobals.lua", none), ("main.lua", main)]).unwrap();
    let lt = e.frame(0.0, 3).unwrap();
    assert_eq!(lt.rows[15].bg[0].scroll_x, 4);
    assert_eq!(lt.rows[15].m7.a, lt.rows[0].m7.a);
}
