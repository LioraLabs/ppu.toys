//! The Luau authoring surface in the preview: modules, routines, lookups, bakes and record contracts.
use ppu_core::LuaEngine;

/// Run `frames` frames and return OAM x of slots 0..n after the last one.
fn run(files: &[(&str, &str)], frames: u32, n: usize) -> Vec<i32> {
    let mut engine = LuaEngine::new();
    engine.set_sources(files).unwrap();
    for f in 0..frames {
        engine.frame(f as f64 / 60., f).unwrap();
    }
    (0..n).map(|i| engine.memory().oam[i].x as i32).collect()
}

fn error(files: &[(&str, &str)], frames: u32) -> String {
    let mut engine = LuaEngine::new();
    if let Err(e) = engine.set_sources(files) {
        return e.message;
    }
    for f in 0..frames {
        if let Err(e) = engine.frame(f as f64 / 60., f) {
            return e.message;
        }
    }
    panic!("expected an error")
}

#[test]
fn modules_run_once_on_first_require_and_keep_identity() {
    let main = r#"local A = require("./state")
local B = require("./state")
function frame(t, f)
  obj[0].x = A.loads
  obj[1].x = if A == B then 1 else 0
end"#;
    let state = "loads = (loads or 0) + 1\nreturn {loads = loads}";
    assert_eq!(run(&[("main.luau", main), ("state.luau", state)], 1, 2), [1, 1]);
}

#[test]
fn routine_waits_resume_on_the_nth_following_step() {
    // Start before update 0's step; yield(2) resumes in update 2; the body ends there.
    let main = r#"local log = 0
local r = routine(function(n: number)
  log = 1
  yield(2)
  log = n
end)
function frame(t, f)
  if f == 0 then
    obj[2].x = if r:start(7) then 1 else 0
    obj[3].x = if r:start(9) then 1 else 0
  end
  r:step()
  obj[0].x = log
  obj[1].x = if r:running() then 1 else 0
end"#;
    assert_eq!(run(&[("main.luau", main)], 1, 4), [1, 1, 1, 0]);
    assert_eq!(run(&[("main.luau", main)], 2, 2), [1, 1]);
    assert_eq!(run(&[("main.luau", main)], 3, 2), [7, 0]);
}

#[test]
fn routine_yields_inside_helpers_and_cancel_discards_it() {
    let main = r#"local count = 0
local function wait_twice() yield() yield() end
local r = routine(function()
  for i = 1, 3 do count += 1; wait_twice() end
end)
function frame(t, f)
  if f == 0 then r:start() end
  if f == 4 then r:cancel() end
  r:step()
  obj[0].x = count
end"#;
    assert_eq!(run(&[("main.luau", main)], 3, 1), [2]);
    assert_eq!(run(&[("main.luau", main)], 9, 1), [2]);
    assert!(error(&[("main.luau", "function frame() yield() end")], 1).contains("inside a routine"));
}

#[test]
fn lookups_select_the_nearest_sample_and_reject_outside_inputs() {
    let main = r#"local f = lookup({x = {0, 20, 4}, k = {1, 2}}, function(x, k) return x * k end)
function frame(t, n)
  obj[0].x = f(5.9, 1)
  obj[1].x = f(6, 2)
  obj[2].x = f(20, 2)
end"#;
    assert_eq!(run(&[("main.luau", main)], 1, 3), [4, 16, 40]);
    let outside = "local f = lookup({x = {0, 3}}, function(x) return x end)\nfunction frame() obj[0].x = f(4) end";
    assert!(error(&[("main.luau", outside)], 1).contains("outside 0..3"));
}

#[test]
fn record_fields_keep_their_constructor_contracts() {
    let main = r#"local function make() return {x = makeFixed(0, -4, 4), hp = makeBounded(2, 0, 2), on = false} end
local list = {}
for _ = 1, 2 do table.insert(list, make()) end
function frame(t, f)
  for _, s in list do s.x += 1.0004 end
  local first = list[1]
  obj[0].x = math.round(first.x * 1024)
  obj[1].x = #list
end"#;
    // 1.0004 snaps to the nearest 1/1024.
    assert_eq!(run(&[("main.luau", main)], 1, 2), [1024, 2]);
    assert!(error(&[("main.luau", main)], 5).contains("-4..4"));
    let hp = "local s = {hp = makeBounded(2, 0, 2)}\nfunction frame() s.hp = 1.5 end";
    assert!(error(&[("main.luau", hp)], 1).contains("integer"));
}

#[test]
fn baked_scanline_programs_run_with_sampled_inputs() {
    let main = r#"local lines = bakeHdma({p = {0, 7}}, 0, 223, function(y, p) bg[1].scroll.x = y + p end)
function frame(t, f) lines(3.4) end"#;
    let mut engine = LuaEngine::new();
    engine.set_sources(&[("main.luau", main)]).unwrap();
    let lines = engine.frame(0., 0).unwrap();
    // 3.4 samples to 3 on every scanline.
    assert_eq!(lines.rows[0].bg[0].scroll_x, 3);
    assert_eq!(lines.rows[100].bg[0].scroll_x, 103);
}
