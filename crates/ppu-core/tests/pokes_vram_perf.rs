//! Frame-time and reload cost of painted tiles (`__ppu_pokes_vram()`).
use ppu_core::LuaEngine;
use std::time::Instant;

fn controls(words: usize) -> String {
    let mut body = String::new();
    for row in 0..words / 32 {
        let ws: Vec<String> = (0..32)
            .map(|i| format!("0x{:x}", (row * 32 + i) & 0x3ff))
            .collect();
        body += &format!(
            "  vr(0x{:04x}, {{ {} }})\n",
            0x4000 + row * 32,
            ws.join(", ")
        );
    }
    let vram = if words > 0 {
        format!("function __ppu_pokes_vram()\n{body}end\n\n")
    } else {
        String::new()
    };
    format!("scanlines = {{\n}}\n\n{vram}function __ppu_pokes()\n  brightness = 7\nend\n")
}

fn per_frame_ms(words: usize) -> f64 {
    let mut e = LuaEngine::new();
    let c = controls(words);
    e.set_sources(&[("main.lua", "function frame(t, f) end\n")])
        .unwrap();
    e.set_pokes(&c).unwrap();
    for f in 0..5 {
        e.frame(0.0, f).unwrap();
    }
    let t = Instant::now();
    for f in 0..60 {
        e.frame(0.0, f).unwrap();
    }
    t.elapsed().as_secs_f64() * 1000.0 / 60.0
}

/// Painted tiles must not cost frame time: they are captured once per load
/// and overlaid in Rust. Before that, 1024 words cost ~20 ms EVERY frame
/// through the tracked Lua proxy. Generous bound so a loaded CI box passes.
#[test]
fn painted_tiles_cost_no_frame_time() {
    let base = per_frame_ms(0);
    let full = per_frame_ms(4096);
    println!("0 words {base:.3} ms/frame, 4096 words {full:.3} ms/frame");
    assert!(
        full < base * 3.0 + 2.0,
        "4096 painted words cost {full:.2} ms/frame (baseline {base:.2})"
    );
}

/// A paint stroke reloads the pokes chunk in place; that one-off must stay
/// well inside a frame even for a big map.
#[test]
fn a_stroke_reload_is_cheap() {
    let mut e = LuaEngine::new();
    let main = "function frame(t, f) end\n";
    let a = controls(4096);
    let b = a.replacen("0x0", "0x1", 1);
    e.set_sources(&[("main.lua", main)]).unwrap();
    e.set_pokes(&a).unwrap();
    e.frame(0.0, 0).unwrap();
    let t = Instant::now();
    for i in 0..20 {
        let src = if i % 2 == 0 { &b } else { &a };
        e.set_pokes(src).unwrap();
    }
    let ms = t.elapsed().as_secs_f64() * 1000.0 / 20.0;
    println!("reload with 4096 words: {ms:.3} ms");
    assert!(ms < 50.0, "reloading 4096 painted words took {ms:.1} ms");
}
