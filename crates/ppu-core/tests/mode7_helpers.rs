use ppu_core::LuaEngine;

#[test]
fn views_are_pure_centered_and_perspective_correct() {
    let mut e = LuaEngine::new();
    e.set_source(
        r#"
local before = m7.a
local flat = mode7_transform(90, 2, 512, 256)
assert(abs(flat.a) < 0.00001 and flat.b == -0.5)
assert(flat.c == 0.5 and abs(flat.d) < 0.00001)
assert(flat.cx == 512 and flat.cy == 256)
assert(flat.scroll_x == 384 and flat.scroll_y == 144)
assert(m7.a == before)
local far = mode7_floor(96, 80, 64, 0, 512, 0)
local near = mode7_floor(208, 80, 64, 0, 512, 0)
assert(far.a == 4 and far.cy == 512)
assert(near.a == 0.5 and near.cy == 64)
assert(far.b == 0 and far.c == 0 and far.d == 0)
assert(far.scroll_x == far.cx - 128)
assert(not mode7_floor(80, 80, 64, 0, 512, 0).visible)
assert(mode7_floor(81, 80, 512, 0, 512, 0).a == 127)
local turn = mode7_floor(208, 80, 64, 90, 512, 0)
assert(abs(turn.a) < 0.00001 and turn.c == 0.5)
assert(turn.cx == 448 and turn.cy == 0)
local wrap = mode7_transform(0, 1, -1, 1025)
assert(wrap.cx == 1023 and wrap.cy == 1)
function frame()
  mode = 7
  hdma(0, 223, function(y)
    local view = mode7_floor(y, 80, 64, 0, 512, 0)
    m7.a = view.a; m7.b = view.b; m7.c = view.c; m7.d = view.d
    m7.cx = view.cx; m7.cy = view.cy
    bg[1].scroll.x = view.scroll_x; bg[1].scroll.y = view.scroll_y
    screen.main.bg1 = view.visible
  end)
end
"#,
    )
    .unwrap();
    let lines = e.frame(0.0, 0).unwrap();
    assert_eq!(lines.rows[96].m7.a, 1024);
    assert_eq!(lines.rows[208].m7.a, 128);
    assert_eq!(lines.rows[208].m7.cy, 64);
}

#[test]
fn invalid_view_inputs_fail_clearly() {
    for source in [
        "mode7_transform(0, 0, 0, 0)",
        "mode7_floor(100, 80, 0, 0, 0, 0)",
        "mode7_transform(0/0, 1, 0, 0)",
        "mode7_floor(100, 80, 64, 0, math.huge, 0)",
    ] {
        assert!(LuaEngine::new().set_source(source).is_err(), "{source}");
    }
}
