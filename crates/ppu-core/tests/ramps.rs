use ppu_core::LuaEngine;

#[test]
fn built_in_ramps_work_without_generated_files_at_load_time_and_inside_hdma() {
    let mut engine = LuaEngine::new();
    engine
        .set_source(
            r#"
assert(clamp(-1, 0, 15) == 0)
assert(clamp(20, 0, 15) == 15)
assert(clamp(7.5, 0, 15) == 7.5)
assert(clamp(9, 3, 3) == 3)
assert(lerp(20, 200, 0) == 20)
assert(lerp(20, 200, 1) == 200)
assert(lerp(20, 200, 0.5) == 110)
assert(lerp(20, 200, 2) == 380)
assert(lerp(20, 200, -1) == -160)
assert(lerp_color(rgb(255,0,0), rgb(0,255,0), 0.5) == 528)
assert(lerp_color(31, 992, -1) == 31)
assert(lerp_color(31, 992, 2) == 992)
assert(lerp_color(0, 32767, 0.5) == 16912)
assert(ease(0.25) == 0.25)
assert(ease(0.25, "smooth") == 0.15625)
assert(ease(0.25, "ease-in") == 0.0625)
assert(ease(0.25, "ease-out") == 0.4375)
assert(ease(0.25, "step") == 0)
assert(ease(1, "step") == 1)
assert(ramp({{10,2},{20,4}}, 0) == 2)
assert(ramp({{10,2},{20,4}}, 30) == 4)
assert(ramp({{10,2},{20,4}}, 12.5) == 2.5)
assert(ramp({{0,0,"smooth"},{1,1}}, 0.25) == 0.15625)
assert(ramp({{0,0,"step"},{1,1}}, 0.5) == 0)
assert(ramp({{0,0,"step"},{1,1}}, 1) == 1)
assert(ramp({{10,3}}, 100) == 3)
assert(ramp_int({{0,-2},{2,1}}, 1) == 0)
assert(ramp_color({{0,31},{2,992}}, 1) == 528)
assert(ramp_color({{10,31},{20,992}}, 0) == 31)
assert(ramp_color({{10,31},{20,992}}, 30) == 992)
assert(ramp_color({{10,31}}, 0) == 31)
local keys = {{0,0},{223,15}}
function frame(t)
  hdma(0, 223, function(y)
    brightness = ramp_int(keys, y)
    m7.a = ramp({{0,1},{223,2}}, y)
    cgram[0] = ramp_color({{0,31},{223,992}}, y)
  end)
end
"#,
        )
        .unwrap();
    let lines = engine.frame(0.0, 0).unwrap();
    for (y, brightness, scale, color) in [(0, 0, 256, 31), (223, 15, 512, 992)] {
        let line = ppu_core::inspect_scanline(&lines, engine.memory(), y).unwrap();
        let register = |addr| {
            line.registers
                .iter()
                .find(|r| r.addr == addr)
                .unwrap()
                .value
        };
        assert_eq!(register(0x2100), brightness);
        assert_eq!(register(0x211b), scale);
        assert_eq!(line.cgram[0], color);
    }
}

#[test]
fn empty_ramps_report_a_useful_error() {
    for helper in ["ramp", "ramp_int", "ramp_color"] {
        let mut engine = LuaEngine::new();
        let error = engine
            .set_source(&format!("{helper}({{}}, 0)"))
            .unwrap_err();
        assert!(error.message.contains("at least one keyframe"), "{error:?}");
    }
}

#[test]
fn clamp_rejects_reversed_bounds() {
    let mut engine = LuaEngine::new();
    let error = engine.set_source("clamp(5, 10, 0)").unwrap_err();
    assert!(error.message.contains("minimum must not exceed maximum"));
}
