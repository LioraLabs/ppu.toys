use ppu_core::LuaEngine;

#[test]
fn signed_integer_division_and_modulo_follow_lua_without_overflow() {
    let mut e = LuaEngine::new();
    e.set_source(
        r#"
function frame()
  local a, b = -7, 3
  assert(a//b == -3 and a%b == 2)
  assert(b//a == -1 and b%a == -4)
  assert(-7//3 == -3 and 7//-3 == -3)
  local min = -9223372036854775807 - 1
  assert(min//-1 == min and min%-1 == 0)
  local max = 9223372036854775807
  assert((max-1)%max == max-1)
  assert((min+1)%min == min+1)
end
"#,
    )
    .unwrap();
    e.frame(0., 0).unwrap();
}

#[test]
fn parallel_assignment_snapshots_values_and_indexed_targets() {
    let mut e = LuaEngine::new();
    e.set_source(
        r#"
local up = 7
function values() return 4, 8, 12 end
function frame()
  local a, b = 3, 5
  a, b = b, a
  assert(a == 5 and b == 3)
  local t, i = {}, 1
  i, t[i] = 2, 42
  assert(i == 2 and t[1] == 42 and t[2] == nil)
  a, b, up = values()
  assert(a == 4 and b == 8 and up == 12)
  a, b = 9
  assert(a == 9 and b == nil)
  a, a = 1, 2
  assert(a == 1)
  t.x, t.x = 1, 2
  assert(t.x == 1)
  local old = t
  t, t.x = {}, 33
  assert(old.x == 33 and t.x == nil)
  g, h = 10, 20
  g, h = h, g
  assert(g == 20 and h == 10)
end
"#,
    )
    .unwrap();
    e.frame(0., 0).unwrap();
}
