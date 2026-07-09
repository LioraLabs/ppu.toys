//! DSL-level guards: a fractional register write lands as an absolute integer in
//! the LineTable (quantize-on-write), while the Lua global keeps the float the
//! user wrote (write-only latch — accumulation across frames still works). Also
//! guards mode/brightness mask-vs-clamp wrapping.
use ppu_core::LuaEngine;

#[test]
fn fractional_scroll_write_is_quantized_in_the_linetable() {
    let mut e = LuaEngine::new();
    e.set_source("function frame(t, f) bg[1].scroll.x = 10.7 end")
        .unwrap();
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
fn binding_registers_quantize_on_write_via_dsl() {
    let mut e = LuaEngine::new();
    e.set_source(
        "function frame(t,f) bg[1].tile_size=16; bg[1].map_base=0x07ff; \
         bg[1].screen_size=5; bg[1].char_base=0x1fff; m7.wrap=6; m7.flip_x=true end",
    )
    .unwrap();
    let lt = e.frame(0.0, 0).unwrap();
    let b = &lt.rows[0].bg[0];
    assert_eq!(b.tile_size, 16);
    assert_eq!(b.map_base, 0x0400); // snapped down to the 0x400-word step
    assert_eq!(b.screen_size, 1); // 5 & 3
    assert_eq!(b.char_base, 0x1000); // snapped down to the 0x1000-word step
    assert_eq!(lt.rows[0].m7.repeat, 2); // m7.wrap=6 -> repeat 6 & 3
    assert!(lt.rows[0].m7.flip_x && !lt.rows[0].m7.flip_y);
}

#[test]
fn mode_and_brightness_wrap_not_clamp() {
    // Locked decision: out-of-range register writes WRAP (mask), not clamp.
    let mut e = LuaEngine::new();
    e.set_source("function frame(t, f) mode = 8; brightness = 20 end")
        .unwrap();
    let lt = e.frame(0.0, 0).unwrap();
    assert_eq!(lt.rows[0].mode, 0); // 8 & 7 = 0 (NOT clamped to 7)
    assert_eq!(lt.rows[0].brightness, 4); // 20 & 0x0f = 4 (NOT clamped to 15)
}

#[test]
fn color_math_registers_bind_through_dsl() {
    let mut e = LuaEngine::new();
    e.set_source("function frame(t,f) CGWSEL=0xC2; CGADSUB=0x41; COLDATA=rgb(255,0,255) end")
        .unwrap();
    let lt = e.frame(0.0, 0).unwrap();
    assert_eq!(lt.rows[0].cgwsel, 0xC2);
    assert_eq!(lt.rows[0].cgadsub, 0x41);
    // rgb(255,0,255) packs to BGR555 with r=31,b=31.
    assert_eq!(lt.rows[0].coldata, (31 << 10) | 31);
}

#[test]
fn mosaic_dsl_maps_global_size_and_per_bg_enable() {
    // Global `mosaic = N` sets the block size; per-layer `bg[n].mosaic = true`
    // enables that BG (n = 1..4 in the DSL, index n-1 internally).
    let mut e = LuaEngine::new();
    e.set_source("function frame(t,f) mosaic = 3; bg[1].mosaic = true end")
        .unwrap();
    let lt = e.frame(0.0, 0).unwrap();
    let regs = &lt.rows[0]; // LineTable rows are already quantized RegRow values
    assert_eq!(regs.mosaic_size, 3);
    assert_eq!(regs.bg[0].mosaic, 4); // BG1 enabled -> size+1
    assert_eq!(regs.bg[1].mosaic, 1); // BG2 not enabled -> off
}

#[test]
fn coldata_helper_accumulates_channel_writes() {
    // Two $2132-style byte writes: red then blue.
    // COLDATA byte: bit5=R, bit6=G, bit7=B, bits0-4=value.
    let mut e = LuaEngine::new();
    e.set_source("function frame(t,f) coldata(0x20|31); coldata(0x80|31) end")
        .unwrap();
    let lt = e.frame(0.0, 0).unwrap();
    // red channel = 31, blue channel = 31, green untouched (0).
    assert_eq!(lt.rows[0].coldata, (31 << 10) | 31);
}
