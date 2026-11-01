// A Lua function called with fewer arguments than parameters must see nil in
// every missing parameter, whatever the caller's registers held before.
// The `do local d1, d2, ... = "s1", ... end` lines leave non-nil values in the
// registers that a missing parameter would otherwise read.
use ppu_core::LuaEngine;

fn run(src: &str) {
    let mut e = LuaEngine::new();
    e.set_source(src).unwrap();
    e.frame(0., 0).unwrap();
}

#[test]
fn plain_call() {
    run(r#"
local function g(a, b, c) return b, c end
function frame()
  do local d1, d2, d3, d4, d5 = "s1", "s2", "s3", "s4", "s5" end
  local b, c = g("p")
  assert(b == nil and c == nil, "plain")
end
"#);
}

#[test]
fn call_in_table_constructor() {
    run(r#"
local function g(a, b) if b ~= nil and type(b) ~= "table" then error("stale opts: " .. tostring(b)) end return 1 end
function frame()
  local s = { a = { g("pluck", { vol = 40 }), 5, "C6" }, b = { g("hat"), 6, "C5" } }
  assert(s.b[1] == 1 and s.b[3] == "C5")
end
"#);
}

#[test]
fn nested_call() {
    run(r#"
local function g(a, b) return b == nil and "nil" or "stale" end
local function f(x, y) return x, y end
function frame()
  do local d1, d2, d3, d4, d5 = "s1", "s2", "s3", "s4", "s5" end
  local x, y = f(g("x"))
  assert(x == "nil" and y == nil, "nested")
  local t = { f(g("x")), "tail" }
  assert(t[1] == "nil")
end
"#);
}

#[test]
fn method_call() {
    run(r#"
local obj = {}
function obj:m(a, b, c) return a, b, c end
function frame()
  do local d1, d2, d3, d4, d5 = "s1", "s2", "s3", "s4", "s5" end
  local a, b, c = obj:m("x")
  assert(a == "x" and b == nil and c == nil, "method")
  local r = { obj:m("y"), "after" }
  assert(r[1] == "y")
  local _, b2, c2 = obj:m("z")
  assert(b2 == nil and c2 == nil)
end
"#);
}

#[test]
fn call_after_call_filled_same_register() {
    run(r#"
local function g(a, b, c) return b, c end
function frame()
  local b, c = g("a", "full-b", "full-c")
  assert(b == "full-b" and c == "full-c")
  do local d1, d2, d3, d4, d5 = "s1", "s2", "s3", "s4", "s5" end
  b, c = g("a")
  assert(b == nil and c == nil, "after full call")
end
"#);
}

// The next four tests guard call paths that already start the callee with an
// exact stack length, so they pass both before and after the fix.
#[test]
fn call_metamethod() {
    run(r#"
local obj = setmetatable({}, { __call = function(self, a, b, c) return b, c end })
function frame()
  do local d1, d2, d3, d4, d5 = "s1", "s2", "s3", "s4", "s5" end
  local b, c = obj("p", "pb", "pc")
  assert(b == "pb" and c == "pc")
  do local d1, d2, d3, d4, d5 = "s1", "s2", "s3", "s4", "s5" end
  b, c = obj("p")
  assert(b == nil and c == nil, "__call")
end
"#);
}

#[test]
fn index_function_extra_param() {
    // __index is called as f(table, key); the extra params are the omitted ones.
    run(r#"
local t = setmetatable({}, { __index = function(self, k, extra, more) return extra == nil and more == nil end })
function frame()
  do local d1, d2, d3, d4, d5 = "s1", "s2", "s3", "s4", "s5" end
  assert(t.k == true, "__index")
  assert(t.j == true, "__index again")
end
"#);
}

#[test]
fn pcall_callback_to_closure() {
    run(r#"
local function g(a, b, c) return b, c end
function frame()
  do local d1, d2, d3, d4, d5 = "s1", "s2", "s3", "s4", "s5" end
  local ok, b, c = pcall(g, "a")
  assert(ok and b == nil and c == nil, "pcall")
end
"#);
}

#[test]
fn generic_for_iterator_extra_param() {
    // The iterator is called with (state, control); a third param is omitted.
    run(r#"
local seen = 0
local function iter(s, c, extra, e2, e3)
  assert(e2 == nil and e3 == nil, "for iterator e2/e3 " .. tostring(e2) .. tostring(e3))
  assert(extra == nil, "for iterator " .. tostring(extra))
  if c < 2 then return c + 1 end
end
function frame()
  do local d1, d2, d3, d4, d5, d6, d7, d8 = "s1", "s2", "s3", "s4", "s5", "s6", "s7", "s8" end
  for i in iter, "state", 0 do seen = seen + 1 end
  assert(seen == 2)
end
"#);
}

#[test]
fn tail_call_short_stack() {
    run(r#"
local function g(a, b, c, d, e, h, i, j) return a, j end
local function f() return g(1) end
function frame()
  do local d1, d2, d3, d4, d5 = "s1", "s2", "s3", "s4", "s5" end
  local a, j = f()
  assert(a == 1 and j == nil, "tail call")
end
"#);
}
