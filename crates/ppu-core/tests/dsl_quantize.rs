//! DSL-level guards: a fractional register write lands as an absolute integer in
//! the LineTable (quantize-on-write), while the Lua global keeps the float the
//! user wrote (write-only latch — accumulation across frames still works). Also
//! guards mode/brightness mask-vs-clamp wrapping.
use ppu_core::LuaEngine;

#[test]
fn fractional_scroll_write_is_quantized_in_the_linetable() {
    let mut e = LuaEngine::new();
    e.set_source("function frame(t, f) bg[1].scroll.x = 10.7 end").unwrap();
    let lt = e.frame(0.0, 0).unwrap();
    assert_eq!(lt.rows[0].bg[0].scroll_x, 11); // rounded, absolute
}

#[test]
fn scroll_global_keeps_the_float_across_frames() {
    // If read-back returned the quantized register (0 each frame), this never
    // moves. The write-only float latch keeps the accumulator alive: after 10
    // frames of +0.3 the global is 3.0 -> quantizes to 3.
    let mut e = LuaEngine::new();
    e.set_source("function frame(t, f) bg[1].scroll.x = bg[1].scroll.x + 0.3 end")
        .unwrap();
    let mut last = 0;
    for f in 0..10 {
        last = e.frame(0.0, f).unwrap().rows[0].bg[0].scroll_x;
    }
    assert_eq!(last, 3); // 0.3*10 = 3.0 -> round 3 (NOT stuck at 0)
}

#[test]
fn mode_and_brightness_wrap_not_clamp() {
    // Locked decision: out-of-range register writes WRAP (mask), not clamp.
    let mut e = LuaEngine::new();
    e.set_source("function frame(t, f) mode = 8; brightness = 20 end").unwrap();
    let lt = e.frame(0.0, 0).unwrap();
    assert_eq!(lt.rows[0].mode, 0); // 8 & 7 = 0 (NOT clamped to 7)
    assert_eq!(lt.rows[0].brightness, 4); // 20 & 0x0f = 4 (NOT clamped to 15)
}
