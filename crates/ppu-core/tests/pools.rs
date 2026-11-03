//! `@ppu/pools` in the preview, and the standard `coroutine.wrap` generators it is written with.
use ppu_core::LuaEngine;

/// Run `frames` frames and return OAM x of slots 0..n after the last one.
fn run(files: &[(&str, &str)], frames: u32, n: usize) -> Vec<i32> {
    let mut engine = LuaEngine::new();
    engine.set_sources(files).unwrap();
    for f in 0..frames {
        engine.frame(f as f64 / 60., f).unwrap_or_else(|e| panic!("frame {f}: {}", e.message));
    }
    (0..n).map(|i| engine.memory().oam[i].x as i32).collect()
}

#[test]
fn a_coroutine_wrap_generator_drives_a_generic_for() {
    let main = r#"local function evens(n: number)
  return coroutine.wrap(function()
    for i = 1, n do
      if i % 2 == 0 then coroutine.yield(i, i * 10) end
    end
  end)
end
function frame(t, f)
  local k = 0
  for i, ten in evens(7) do
    obj[k].x = i + ten
    k += 1
  end
  obj[3].x = k
end"#;
    assert_eq!(run(&[("main.luau", main)], 1, 4), [22, 44, 66, 3]);
}

const POOL_HEAD: &str = r#"local Pools = require("@ppu/pools")
local function make() return {v = 0} end
local pool = Pools.new(3, make)
"#;

fn pool_run(body: &str, n: usize) -> Vec<i32> {
    run(&[("main.luau", &format!("{POOL_HEAD}function frame(t, f)\n{body}\nend"))], 1, n)
}

#[test]
fn spawn_fills_the_pool_then_returns_nil_and_free_reuses_the_record() {
    let body = r#"local a = pool:spawn()
local b = pool:spawn()
local c = pool:spawn()
obj[0].x = if pool:spawn() == nil then 1 else 0
obj[1].x = if a ~= b and b ~= c and a ~= c then 1 else 0
b.v = 7
pool:free(b)
local again = pool:spawn()
obj[2].x = if again == b then 1 else 0
obj[3].x = again.v
obj[4].x = if pool:spawn() == nil then 1 else 0"#;
    assert_eq!(pool_run(body, 5), [1, 1, 1, 7, 1]);
}

#[test]
fn live_visits_live_records_in_pool_order_and_break_stops() {
    let body = r#"local a = pool:spawn(); a.v = 10
local b = pool:spawn(); b.v = 20
local c = pool:spawn(); c.v = 30
pool:free(b)
local k = 0
for s in pool:live() do obj[k].x = s.v; k += 1 end
obj[2].x = k
local j = 0
for s in pool:live() do j += 1; break end
obj[3].x = j"#;
    assert_eq!(pool_run(body, 4), [10, 30, 2, 1]);
}

#[test]
fn freeing_or_spawning_inside_live_behaves_as_plain_lua() {
    // Pool order a,b,c. Freeing the current record: it is not visited again; later ones still are.
    let free_body = r#"for i = 1, 3 do pool:spawn().v = i end
local k = 0
for s in pool:live() do
  pool:free(s)
  obj[k].x = s.v
  k += 1
end
obj[3].x = k
obj[4].x = if pool:spawn() ~= nil then 1 else 0"#;
    assert_eq!(pool_run(free_body, 5), [1, 2, 3, 3, 1]);
    // Spawning a free slot ahead of the cursor: the new record is visited; one behind it is not.
    // Live: a(slot 1) and c(slot 3); slot 2 is free. Visiting a spawns slot 2, which is then visited.
    let spawn_body = r#"local a = pool:spawn(); a.v = 1
local b = pool:spawn(); b.v = 2
local c = pool:spawn(); c.v = 3
pool:free(b)
local k = 0
for s in pool:live() do
  if s == a then pool:spawn().v = 9 end
  obj[k].x = s.v
  k += 1
end
obj[3].x = k"#;
    assert_eq!(pool_run(spawn_body, 4), [1, 9, 3, 3]);
}

#[test]
fn pools_of_different_records_are_independent() {
    let main = r#"local Pools = require("@ppu/pools")
local function make_shot() return {x = 1} end
local function make_foe() return {hp = 5, y = 2} end
local shots = Pools.new(2, make_shot)
local foes = Pools.new(1, make_foe)
function frame(t, f)
  shots:spawn(); shots:spawn()
  local foe = foes:spawn()
  obj[0].x = if foes:spawn() == nil then foe.hp else 0
  foes:free(foe)
  obj[1].x = if shots:spawn() == nil then 1 else 0
  local n = 0
  for s in shots:live() do n += s.x end
  obj[2].x = n
  local m = 0
  for e in foes:live() do m += 1 end
  obj[3].x = m
end"#;
    assert_eq!(run(&[("main.luau", main)], 1, 4), [5, 1, 2, 0]);
}
