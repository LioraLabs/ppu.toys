//! Hot swap: a recompile of the running program carries each file-level local that a top-level
//! function captured, runs the new code over the old data, and stays in the running scene.
//! Run (`reset`) and another toy opening (`forget_run`) power on fresh.
mod common;

use common::{add_m7, add_sheet};
use ppu_core::{render_frame, LuaEngine};
use serde_json::json;

fn engine(files: &[(&str, &str)]) -> LuaEngine {
    let mut e = LuaEngine::new();
    e.set_sources(files).unwrap();
    e
}

fn run(e: &mut LuaEngine, frames: std::ops::Range<u32>) {
    for f in frames {
        e.frame(f as f64 / 60.0, f).unwrap();
    }
}

fn ints(e: &mut LuaEngine, name: &str) -> Vec<i64> {
    e.call_ints(name, &[]).unwrap()
}

const COUNTER: &str =
    "local x = 0\nfunction frame(t, f) x = x + STEP end\nfunction probe() return {x} end";

fn counter(step: u32) -> String {
    COUNTER.replace("STEP", &step.to_string())
}

#[test]
fn a_recompile_carries_the_counter_and_runs_the_new_code() {
    let mut e = engine(&[("main.luau", &counter(1))]);
    run(&mut e, 0..30);
    e.set_sources(&[("main.luau", &counter(2))]).unwrap();
    assert_eq!(ints(&mut e, "probe"), vec![30]);
    run(&mut e, 30..31);
    assert_eq!(ints(&mut e, "probe"), vec![32]);
    assert!(e.not_carried().is_empty());
}

#[test]
fn init_reruns_but_loses_to_the_carry() {
    let src = |c: &str| {
        format!("local x = 0\nfunction init() x = 5 end\nfunction frame(t, f) x = x + 1 end -- {c}\nfunction probe() return {{x}} end")
    };
    let mut e = engine(&[("main.luau", &src("a"))]);
    run(&mut e, 0..10);
    assert_eq!(ints(&mut e, "probe"), vec![15]);
    e.set_sources(&[("main.luau", &src("b"))]).unwrap();
    assert_eq!(ints(&mut e, "probe"), vec![15]);
}

fn module(hp: i64, cost: i64) -> String {
    format!("local M = {{hp = {hp}}}\nfunction M.hit() M.hp = M.hp - {cost} end\nreturn M")
}

const MODULE_MAIN: &str = "local m = require(\"./m\")\nfunction frame(t, f) end\nfunction hit() m.hit() return {m.hp} end\nfunction probe() return {m.hp} end";

#[test]
fn a_module_table_keeps_its_data_and_takes_the_new_code() {
    let mut e = engine(&[("main.luau", MODULE_MAIN), ("m.luau", &module(5, 1))]);
    ints(&mut e, "hit");
    ints(&mut e, "hit");
    assert_eq!(ints(&mut e, "probe"), vec![3]);
    e.set_sources(&[("main.luau", MODULE_MAIN), ("m.luau", &module(9, 2))])
        .unwrap();
    assert_eq!(ints(&mut e, "probe"), vec![3], "the old data, not hp = 9");
    ints(&mut e, "hit");
    assert_eq!(ints(&mut e, "probe"), vec![1], "the new hit costs 2");
}

#[test]
fn a_changed_kind_takes_the_new_value_and_is_listed() {
    let src = |init: &str| {
        format!("local s = {init}\nfunction frame(t, f) end\nfunction kind() return {{type(s) == \"table\" and 1 or 0}} end")
    };
    let mut e = engine(&[("main.luau", &src("0"))]);
    run(&mut e, 0..3);
    e.set_sources(&[("main.luau", &src("{}"))]).unwrap();
    assert_eq!(ints(&mut e, "kind"), vec![1]);
    let lines = e.not_carried();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].contains("main.luau") && lines[0].contains(" s "),
        "{lines:?}"
    );
    // The next load starts the list over.
    e.set_sources(&[("main.luau", &src("{}"))]).unwrap();
    assert!(e.not_carried().is_empty());
}

#[test]
fn a_function_in_carried_data_is_the_new_function() {
    let src = |ret: i64| {
        format!("local function f() return {ret} end\nlocal cbs = {{f}}\nfunction frame(t, f) cbs.tag = (cbs.tag or 0) + 1 end\nfunction probe() return {{cbs[1](), cbs.tag or 0}} end")
    };
    let mut e = engine(&[("main.luau", &src(1))]);
    run(&mut e, 0..3);
    assert_eq!(ints(&mut e, "probe"), vec![1, 3]);
    e.set_sources(&[("main.luau", &src(2))]).unwrap();
    assert_eq!(ints(&mut e, "probe"), vec![2, 3]);
}

#[test]
fn a_function_the_new_code_no_longer_has_leaves_the_variable_uncarried() {
    let old = "local function f() return 1 end\nlocal cbs = {f}\nfunction frame(t, f) cbs.n = 5 end\nfunction probe() return {#cbs, cbs.n or 0} end";
    let new =
        "local cbs = {}\nfunction frame(t, f) end\nfunction probe() return {#cbs, cbs.n or 0} end";
    let mut e = engine(&[("main.luau", old)]);
    run(&mut e, 0..2);
    e.set_sources(&[("main.luau", new)]).unwrap();
    assert_eq!(ints(&mut e, "probe"), vec![0, 0]);
    let lines = e.not_carried();
    assert!(
        lines.len() == 1 && lines[0].contains("cbs") && lines[0].contains("function"),
        "{lines:?}"
    );
}

const SCENE_MAIN: &str = "scenes.add(require(\"./a\"))\nB = scenes.add(require(\"./b\"))";
const SCENE_A: &str =
    "local function frame(t, f) if f == 2 then scenes.go(B) end end\nreturn {frame = frame}";

fn scene_b(step: u32) -> String {
    format!("local n = 0\nlocal seen = 0\nlocal function frame(t, f) n = n + {step}; seen = f end\nfunction probe_b() return {{n, seen}} end\nreturn {{frame = frame}}")
}

#[test]
fn a_recompile_stays_in_the_running_scene() {
    let mut e = engine(&[
        ("main.luau", SCENE_MAIN),
        ("a.luau", SCENE_A),
        ("b.luau", &scene_b(1)),
    ]);
    run(&mut e, 0..11);
    // go() on update 2 lands on 3: the second scene ran its updates 3..=10.
    assert_eq!(
        (e.current_scene(), ints(&mut e, "probe_b")),
        (1, vec![8, 7])
    );
    e.set_sources(&[
        ("main.luau", SCENE_MAIN),
        ("a.luau", SCENE_A),
        ("b.luau", &scene_b(2)),
    ])
    .unwrap();
    assert_eq!(e.current_scene(), 1);
    run(&mut e, 11..12);
    assert_eq!(
        ints(&mut e, "probe_b"),
        vec![10, 8],
        "n kept and +2; f still counts from the entry"
    );
    // Run powers on in the first scene.
    e.reset().unwrap();
    assert_eq!(e.current_scene(), 0);
    assert_eq!(ints(&mut e, "probe_b"), vec![0, 0]);
}

#[test]
fn reset_powers_on_with_the_initial_values() {
    let mut e = engine(&[("main.luau", &counter(1))]);
    run(&mut e, 0..30);
    e.set_sources(&[("main.luau", &counter(2))]).unwrap();
    e.reset().unwrap();
    assert_eq!(ints(&mut e, "probe"), vec![0]);
}

#[test]
fn another_toy_opening_inherits_nothing() {
    let mut e = engine(&[("main.luau", &counter(1))]);
    run(&mut e, 0..30);
    e.forget_run();
    e.set_sources(&[("main.luau", &counter(1))]).unwrap();
    assert_eq!(ints(&mut e, "probe"), vec![0]);
    assert!(e.not_carried().is_empty());
}

const INVARIANT_MAIN: &str = r#"
local m = require("./m")
local x = 0
local tint = 0
function init() bg[1].scroll.y = 3 end
function frame(t, f)
  x = x + 1
  m.tick()
  tint = (tint + 3) % 32
  bg[1].scroll.x = x
  cgram[0] = tint * 32 + m.n
end
function probe() return {x, m.n, tint} end
"#;
const INVARIANT_M: &str = "local M = {n = 0}\nfunction M.tick() M.n = (M.n + 2) % 29 end\nreturn M";

/// Run `files` for 60 frames straight and again with an unchanged recompile at frame 30: every
/// frame's pixels and the probe must match.
fn assert_recompile_invisible(files: &[(&str, &str)], probe: &str) {
    let mut a = engine(files);
    let mut b = engine(files);
    let shots = |e: &mut LuaEngine, range: std::ops::Range<u32>| -> Vec<Vec<u8>> {
        range
            .map(|f| {
                let lt = e.frame(f as f64 / 60.0, f).unwrap();
                render_frame(&lt, e.memory())
            })
            .collect()
    };
    let want = shots(&mut a, 0..60);
    let mut got = shots(&mut b, 0..30);
    b.set_sources(files).unwrap();
    assert!(b.not_carried().is_empty(), "{:?}", b.not_carried());
    got.extend(shots(&mut b, 30..60));
    for (f, (w, g)) in want.iter().zip(&got).enumerate() {
        assert!(w == g, "frame {f} differs after the recompile");
    }
    assert_eq!(ints(&mut a, probe), ints(&mut b, probe));
}

#[test]
fn recompiling_an_unchanged_program_is_invisible() {
    assert_recompile_invisible(
        &[("main.luau", INVARIANT_MAIN), ("m.luau", INVARIANT_M)],
        "probe",
    );
}

const BLESSED_MAIN: &str = "scenes.add(require(\"./a\"))\nB = scenes.add(require(\"./b\"))";
const BLESSED_B: &str = r#"
local cells = {}
for i = 1, 64 do cells[i] = {tile = (i - 1) * 2} end
local sheet = {tile = 0, pal = 2, cells = cells}
local dot = sprites.art({{size = 16, columns = 1, cells = {1}}}, sheet)
local grp = sprites.group()
local hero = {x = makeFixed(0, 0, 255, 1), y = makeBounded(0, 0, 255)}
local game = {g = grp, n = 0, team = {lead = {z = makeBounded(0, 0, 99)}}}
local function idle() hero.y = (hero.y + 1) % 200 end
local function run() hero.x = (hero.x + 3) % 200 end
local st = idle
local boss = {hp = 3}
local function frame(t, f)
  st()
  if f == 10 then st = run end
  if f == 20 then boss = nil end
  game.n = game.n + 1
  game.team.lead.z = (game.team.lead.z + 2) % 90
  grp:draw(dot, hero.x, hero.y)
  bg[1].scroll.x = hero.x
  cgram[0] = game.n + game.team.lead.z
end
function probe_b()
  return {hero.x, hero.y, game.n, game.team.lead.z, boss and boss.hp or -1, st == run and 1 or 0}
end
return {frame = frame}
"#;

#[test]
fn recompiling_a_program_of_the_blessed_apis_is_invisible() {
    assert_recompile_invisible(
        &[
            ("main.luau", BLESSED_MAIN),
            ("a.luau", SCENE_A),
            ("b.luau", BLESSED_B),
        ],
        "probe_b",
    );
}

#[test]
fn makefixed_and_makebounded_values_carry_as_numbers() {
    let src = |step: u32| {
        format!("local p = makeFixed(0, 0, 100, 1)\nlocal q = makeBounded(0, 0, 100)\nfunction frame(t, f) p = p + {step}; q = q + {step} end\nfunction probe() return {{p, q}} end")
    };
    let mut e = engine(&[("main.luau", &src(1))]);
    run(&mut e, 0..10);
    e.set_sources(&[("main.luau", &src(3))]).unwrap();
    assert_eq!(ints(&mut e, "probe"), vec![10, 10]);
    assert!(e.not_carried().is_empty(), "{:?}", e.not_carried());
}

#[test]
fn a_timer_closures_capture_carries() {
    let src = |step: u32| {
        format!("local beats = 0\nfunction init() timer(0, 32, function() beats = beats + {step} end) end\nfunction frame(t, f) end\nfunction probe() return {{beats}} end")
    };
    let mut e = engine(&[("main.luau", &src(1))]);
    run(&mut e, 0..30);
    let before = ints(&mut e, "probe")[0];
    assert!(before > 0);
    e.set_sources(&[("main.luau", &src(2))]).unwrap();
    assert_eq!(ints(&mut e, "probe"), vec![before]);
}

fn record_src(step: u32, hi: u32) -> String {
    format!("local p = {{x = makeFixed(0, 0, {hi}, 1), y = makeBounded(0, 0, {hi})}}\nfunction frame(t, f) p.x = p.x + {step}; p.y = p.y + {step} * 2 end\nfunction probe() return {{p.x, p.y}} end")
}

#[test]
fn a_record_of_makefixed_fields_carries_its_values() {
    let mut e = engine(&[("main.luau", &record_src(1, 255))]);
    run(&mut e, 0..10);
    assert_eq!(ints(&mut e, "probe"), vec![10, 20]);
    e.set_sources(&[("main.luau", &record_src(2, 255))])
        .unwrap();
    assert_eq!(ints(&mut e, "probe"), vec![10, 20]);
    assert!(e.not_carried().is_empty(), "{:?}", e.not_carried());
    run(&mut e, 10..11);
    assert_eq!(
        ints(&mut e, "probe"),
        vec![12, 24],
        "the new code over the old data"
    );
}

#[test]
fn a_record_keeps_the_new_bounds() {
    let mut e = engine(&[("main.luau", &record_src(1, 255))]);
    run(&mut e, 0..10);
    e.set_sources(&[("main.luau", &record_src(1, 12))]).unwrap();
    run(&mut e, 10..14);
    // y is 20 + 4 * 2 = 28, past the new bound 12: a violation of the NEW contract.
    assert!(
        !e.violations().is_empty(),
        "the new proxy checks the new bounds"
    );
}

#[test]
fn a_nested_record_and_a_sprites_group_leave_the_rest_of_the_variable_carried() {
    let src = |step: u32| {
        format!(
            "local cells = {{}}\nfor i = 1, 8 do cells[i] = {{tile = (i - 1) * 2}} end\nlocal dot = sprites.art({{{{size = 16, columns = 1, cells = {{1}}}}}}, {{tile = 0, pal = 2, cells = cells}})\nlocal grp = sprites.group()\nlocal game = {{g = grp, n = 0, hero = {{x = makeFixed(0, 0, 255, 1)}}}}\nfunction frame(t, f) game.n = game.n + {step}; game.hero.x = game.hero.x + {step}; grp:draw(dot, 40, 50) end\nfunction probe() return {{game.n, game.hero.x}} end"
        )
    };
    let mut e = engine(&[("main.luau", &src(1))]);
    run(&mut e, 0..5);
    e.set_sources(&[("main.luau", &src(3))]).unwrap();
    assert_eq!(ints(&mut e, "probe"), vec![5, 5]);
    assert!(e.not_carried().is_empty(), "{:?}", e.not_carried());
    run(&mut e, 5..6);
    assert_eq!(ints(&mut e, "probe"), vec![8, 8]);
    assert!(e.memory().oam[0].on, "the new group still draws");
    assert_eq!(e.memory().oam[0].x, 32);
}

#[test]
fn a_variable_holding_a_function_keeps_which_function() {
    let src = |c: &str| {
        format!("local function idle() return 1 end\nlocal function run() return 2 end\nlocal st = idle\nfunction frame(t, f) if f == 2 then st = run end end -- {c}\nfunction probe() return {{st()}} end")
    };
    let mut e = engine(&[("main.luau", &src("a"))]);
    run(&mut e, 0..5);
    assert_eq!(ints(&mut e, "probe"), vec![2]);
    e.set_sources(&[("main.luau", &src("b"))]).unwrap();
    assert_eq!(ints(&mut e, "probe"), vec![2]);
    assert!(e.not_carried().is_empty(), "{:?}", e.not_carried());
}

#[test]
fn a_function_variable_the_new_code_lost_takes_the_new_value_and_is_listed() {
    let old = "local function idle() return 1 end\nlocal function run() return 2 end\nlocal st = idle\nfunction frame(t, f) st = run end\nfunction probe() return {st()} end";
    let new = "local function idle() return 1 end\nlocal st = idle\nfunction frame(t, f) end\nfunction probe() return {st()} end";
    let mut e = engine(&[("main.luau", old)]);
    run(&mut e, 0..2);
    e.set_sources(&[("main.luau", new)]).unwrap();
    assert_eq!(ints(&mut e, "probe"), vec![1]);
    let lines = e.not_carried();
    assert!(lines.len() == 1 && lines[0].contains(" st "), "{lines:?}");
}

#[test]
fn a_variable_that_became_nil_stays_nil() {
    let src = |c: &str| {
        format!("local boss = {{hp = 3}}\nfunction frame(t, f) if f == 2 then boss = nil end end -- {c}\nfunction probe() return {{boss and boss.hp or -1}} end")
    };
    let mut e = engine(&[("main.luau", &src("a"))]);
    run(&mut e, 0..4);
    assert_eq!(ints(&mut e, "probe"), vec![-1]);
    e.set_sources(&[("main.luau", &src("b"))]).unwrap();
    assert_eq!(ints(&mut e, "probe"), vec![-1]);
    assert!(e.not_carried().is_empty(), "{:?}", e.not_carried());
}

#[test]
fn a_failed_load_in_a_scene_keeps_the_scene_for_the_fix() {
    let files = |b: &str| {
        vec![
            ("main.luau", SCENE_MAIN.to_string()),
            ("a.luau", SCENE_A.to_string()),
            ("b.luau", b.to_string()),
        ]
    };
    let set = |e: &mut LuaEngine, b: &str| {
        let f = files(b);
        let r: Vec<(&str, &str)> = f.iter().map(|(n, s)| (*n, s.as_str())).collect();
        e.set_sources(&r)
    };
    let mut e = engine(&[
        ("main.luau", SCENE_MAIN),
        ("a.luau", SCENE_A),
        ("b.luau", &scene_b(1)),
    ]);
    run(&mut e, 0..11);
    assert_eq!(e.current_scene(), 1);
    let broken = scene_b(1)
        .replace(
            "local n = 0",
            "local n = 0\nlocal function init() error(\"boom\") end",
        )
        .replace(
            "return {frame = frame}",
            "return {frame = frame, init = init}",
        );
    assert!(set(&mut e, &broken).is_err());
    set(&mut e, &scene_b(1)).unwrap();
    assert_eq!(
        e.current_scene(),
        1,
        "still in the scene the author was editing"
    );
    assert_eq!(ints(&mut e, "probe_b")[0], 8);
}

#[test]
fn engine_setup_handles_are_not_a_program_visible_global() {
    let src = "local p = dma('piano')\nfunction frame(t, f) end\nfunction probe() return {__ppu_setup_handles == nil and 1 or 0} end";
    let mut e = engine(&[("main.luau", src)]);
    assert_eq!(ints(&mut e, "probe"), vec![1]);
}

const SPAWNED: &str = r#"
local bullets = {}
function frame(t, f)
  if f == 0 then bullets[1] = {x = makeFixed(0, 0, 255, 4)} end
  bullets[1].x = bullets[1].x + 0.1
  cgram[0] = math.floor(bullets[1].x * 4)
end
function probe() return {math.floor(bullets[1].x * 4)} end
"#;

#[test]
fn a_record_built_at_runtime_stays_a_record_across_a_recompile() {
    assert_recompile_invisible(&[("main.luau", SPAWNED)], "probe");
}

#[test]
fn a_spawned_record_keeps_recording_violations() {
    let src = "local list = {}\nfunction frame(t, f) if f == 0 then list[1] = {x = makeBounded(0, 0, 9)} end if f > 4 then list[1].x = list[1].x + 40 end end";
    let mut e = engine(&[("main.luau", src)]);
    run(&mut e, 0..3);
    e.set_sources(&[("main.luau", src)]).unwrap();
    assert!(e.violations().is_empty());
    run(&mut e, 3..7);
    assert!(
        !e.violations().is_empty(),
        "the rebuilt proxy still checks stores"
    );
}

fn nil_cb(init: &str) -> String {
    format!("local cb = {init}\nfunction frame(t, f) end\nfunction probe() return {{type(cb) == \"function\" and cb() or (cb == nil and -1 or -2)}} end")
}

#[test]
fn an_old_nil_carries_over_any_new_value() {
    for new in ["function() return 7 end", "5"] {
        let mut e = engine(&[("main.luau", &nil_cb("nil"))]);
        run(&mut e, 0..2);
        e.set_sources(&[("main.luau", &nil_cb(new))]).unwrap();
        assert_eq!(ints(&mut e, "probe"), vec![-1], "nil carries over {new}");
        assert!(e.not_carried().is_empty(), "{:?}", e.not_carried());
    }
}

#[test]
fn a_consumed_one_shot_callback_stays_consumed() {
    assert_recompile_invisible(
        &[(
            "main.luau",
            "local n = 0\nlocal pending = function() n = n + 100 end\nfunction frame(t, f) if pending then pending() pending = nil end n = n + 1 end\nfunction probe() return {n} end",
        )],
        "probe",
    );
}

#[test]
fn a_nested_plain_table_where_the_new_code_has_a_record_is_listed() {
    let src = |slot: &str| {
        format!("local game = {{n = 0, slot = {slot}}}\nfunction frame(t, f) game.n = game.n + 1 end\nfunction probe() return {{game.n, game.slot.x}} end")
    };
    let mut e = engine(&[("main.luau", &src("{x = 1}"))]);
    run(&mut e, 0..3);
    e.set_sources(&[("main.luau", &src("{x = makeFixed(0, 0, 255, 1)}"))])
        .unwrap();
    // The rest carries; the slot keeps the new record (x = 0), as a variable would, and says so.
    assert_eq!(ints(&mut e, "probe"), vec![3, 0]);
    let lines = e.not_carried();
    assert!(
        lines.len() == 1 && lines[0].contains("game") && lines[0].contains("kept its new value"),
        "{lines:?}"
    );
}

#[test]
fn a_replaced_record_proxy_is_refused_not_a_panic() {
    let src = "__ppu_record_proxy = nil\nlocal p = {x = makeFixed(0, 0, 9, 1)}\nfunction frame(t, f) end";
    let mut e = LuaEngine::new();
    let err = e.set_sources(&[("main.luau", src)]).unwrap_err().message;
    assert!(err.contains("__ppu_record_proxy"), "{err}");
}

#[test]
fn a_record_the_carry_cannot_rebuild_is_listed_not_a_panic() {
    let mut e = engine(&[("main.luau", SPAWNED)]);
    run(&mut e, 0..3);
    let src = format!("__ppu_record_proxy = nil\n{SPAWNED}");
    e.set_sources(&[("main.luau", &src)]).unwrap();
    let lines = e.not_carried();
    assert!(
        lines.iter().any(|l| l.contains("__ppu_record_proxy")),
        "{lines:?}"
    );
}

// ---- worlds redraw at the carried camera ----

/// A 64x16 sheet of 16 chars (0 blank), each its own colour and asymmetric pattern.
fn sheet() -> Vec<u8> {
    let mut rgba = vec![0u8; 64 * 16 * 4];
    for c in 1..16usize {
        for py in 0..8 {
            for px in 0..8 {
                if (px * 3 + py * 5 + c) % 4 == 0 {
                    continue;
                }
                let at = (((c / 8) * 8 + py) * 64 + (c % 8) * 8 + px) * 4;
                rgba[at..at + 4].copy_from_slice(&[(c * 16) as u8, 255 - (c * 16) as u8, 64, 255]);
            }
        }
    }
    rgba
}

const COLS: usize = 128;
const ROWS: usize = 28;

/// A 128x28 map; `new` swaps the cells in columns 36..46 for other chars, one of them solid.
fn world_map(new: bool) -> String {
    let data: Vec<usize> = (0..COLS * ROWS)
        .map(|i| {
            let (c, r) = (i % COLS, i / COLS);
            if new && (36..46).contains(&c) {
                9 + (c + r) % 7
            } else {
                1 + (c * 7 + r * 3) % 7
            }
        })
        .collect();
    json!({
        "orientation": "orthogonal", "infinite": false, "tilewidth": 8, "tileheight": 8,
        "width": COLS, "height": ROWS,
        "tilesets": [{
            "firstgid": 1, "name": "cave", "image": "art/tiles.png", "imagewidth": 64, "imageheight": 16,
            "tilewidth": 8, "tileheight": 8, "columns": 8, "tilecount": 16,
            "tiles": [{"id": 8, "type": "solid"}]
        }],
        "layers": [{"type": "tilelayer", "name": "ground", "width": COLS, "height": ROWS, "data": data}]
    })
    .to_string()
}

fn world_main(tag: &str) -> String {
    format!(
        r#"local world = require("@ppu/world")
local tiles = dma("tiles", {{char = 0x2000, pal = 32}})
local w = world.load("level1")
local x = 0
function init()
  mode = 1
  brightness = 15
  bg[1].map_base = 0x0800
  w:bind("ground", bg[1])
  w:follow(0, 0)
  screen.main.bg1 = true
end
function frame(t, f)
  if f > 0 then x = x + 4 end
  w:follow(x, 0)
end
function probe() return {{x, bg[1].scroll.x, w:tile("ground", 42 * 8 + 1, 1) == w.kinds.solid and 1 or 0}} end -- {tag}
"#
    )
}

fn world_engine(main: &str, map: &str) -> LuaEngine {
    let mut e = LuaEngine::new();
    add_sheet(&mut e, "tiles", sheet(), 64, 16, 4);
    e.set_sources(&[("main.luau", main), ("level1.tmj", map)])
        .unwrap();
    e
}

fn shot(e: &mut LuaEngine, f: u32) -> Vec<u8> {
    let lt = e.frame(f as f64 / 60.0, f).unwrap();
    render_frame(&lt, e.memory())
}

#[test]
fn a_swapped_map_redraws_the_window_at_the_carried_camera() {
    let (old, new) = (world_map(false), world_map(true));
    let mut e = world_engine(&world_main("a"), &old);
    run(&mut e, 0..50);
    assert_eq!(ints(&mut e, "probe"), vec![196, 196, 0]);
    e.set_sources(&[("main.luau", &world_main("a")), ("level1.tmj", &new)])
        .unwrap();
    let got = shot(&mut e, 50);
    assert_eq!(&ints(&mut e, "probe")[..2], &[200, 200]);
    let mut fresh = world_engine(&world_main("a"), &new);
    run(&mut fresh, 0..50);
    assert_eq!(
        got,
        shot(&mut fresh, 50),
        "the picture at the carried camera is the new map's"
    );
}

#[test]
fn collision_answers_from_the_new_map_after_a_swap() {
    let mut e = world_engine(&world_main("a"), &world_map(false));
    run(&mut e, 0..50);
    e.set_sources(&[
        ("main.luau", &world_main("a")),
        ("level1.tmj", &world_map(true)),
    ])
    .unwrap();
    run(&mut e, 50..51);
    assert_eq!(ints(&mut e, "probe")[2], 1);
}

#[test]
fn a_main_only_swap_leaves_the_picture_as_it_was() {
    let map = world_map(false);
    let mut e = world_engine(&world_main("a"), &map);
    let mut plain = world_engine(&world_main("a"), &map);
    run(&mut e, 0..50);
    run(&mut plain, 0..50);
    e.set_sources(&[("main.luau", &world_main("b")), ("level1.tmj", &map)])
        .unwrap();
    assert_eq!(shot(&mut e, 50), shot(&mut plain, 50));
}

fn plane_map(new: bool) -> String {
    let (w, h) = (200usize, 300usize);
    let data: Vec<usize> = (0..w * h)
        .map(|i| {
            let (c, r) = (i % w, i / w);
            if new {
                1 + (c * 5 + r * 3) % 7
            } else {
                1 + (c * 7 + r * 13) % 7
            }
        })
        .collect();
    json!({
        "orientation": "orthogonal", "infinite": false, "tilewidth": 8, "tileheight": 8, "width": w, "height": h,
        "tilesets": [{"firstgid": 1, "name": "track", "image": "track.png", "imagewidth": 32, "imageheight": 16,
            "tilewidth": 8, "tileheight": 8, "columns": 4, "tilecount": 8}],
        "layers": [{"type": "tilelayer", "name": "track", "width": w, "height": h, "data": data}]
    })
    .to_string()
}

const PLANE_MAIN: &str = r#"local world = require("@ppu/world")
local plane = dma("track")
local w = world.load("level1", {radius = 20})
local x = 806
function init()
  mode = 7
  w:bind("track", m7)
  w:follow(806, 1006)
end
function frame(t, f)
  x = x + 4
  w:follow(x, 1006)
end
function probe() return {x} end
"#;

#[test]
fn a_streamed_mode_7_world_survives_a_swap_mid_follow() {
    let mut rgba = vec![0u8; 32 * 16 * 4];
    for (i, px) in rgba.chunks_mut(4).enumerate() {
        px.copy_from_slice(&[(i % 32 * 8) as u8, (i / 32 * 40) as u8, 90, 255]);
    }
    let build = |map: &str| {
        let mut e = LuaEngine::new();
        add_m7(&mut e, "track", rgba.clone(), 32, 16);
        e.set_sources(&[("main.luau", PLANE_MAIN), ("level1.tmj", map)])
            .unwrap();
        e
    };
    let mut e = build(&plane_map(false));
    run(&mut e, 0..60);
    e.set_sources(&[("main.luau", PLANE_MAIN), ("level1.tmj", &plane_map(true))])
        .unwrap();
    run(&mut e, 60..63);
    let x = ints(&mut e, "probe")[0] as usize;
    let mut fresh = build(&plane_map(true));
    run(&mut fresh, 0..63);
    let (a, b) = (&e.memory().vram, &fresh.memory().vram);
    for cy in 1006 / 8 - 20..1006 / 8 + 20 {
        for cx in x / 8 - 20..x / 8 + 20 {
            let at = (cy % 128) * 128 + cx % 128;
            assert_eq!(a[at] & 0xff, b[at] & 0xff, "cell {cx},{cy}");
        }
    }
}

// ---- a world that entered another map stays in it ----

const ENTER_MAIN: &str = r#"local world = require("@ppu/world")
local tiles = dma("tiles", {char = 0x2000, pal = 32})
local w = world.load("level1")
local level2 = world.load("level2")
local inside = false
function init()
  mode = 1
  brightness = 15
  bg[1].map_base = 0x0800
  w:bind("ground", bg[1])
  w:follow(0, 0)
  screen.main.bg1 = true
end
function frame(t, f)
  force_blank = false
  if f == 5 then force_blank = true w:enter("level2", 0, 0) inside = true end
  w:follow(0, 0)
end
function probe() return {inside and 1 or 0, w.name == "level2" and 1 or 0} end
"#;

fn enter_engine(main: &str, level2: Option<&str>) -> LuaEngine {
    let (m1, m2) = (world_map(false), world_map(true));
    let mut files = vec![("main.luau", main), ("level1.tmj", m1.as_str())];
    if let Some(_) = level2 {
        files.push(("level2.tmj", m2.as_str()));
    }
    let mut e = LuaEngine::new();
    add_sheet(&mut e, "tiles", sheet(), 64, 16, 4);
    e.set_sources(&files).unwrap();
    e
}

#[test]
fn entered_map_after_unchanged_recompile() {
    let mut never = enter_engine(ENTER_MAIN, Some(""));
    let mut swap = enter_engine(ENTER_MAIN, Some(""));
    run(&mut never, 0..10);
    run(&mut swap, 0..10);
    let (m1, m2) = (world_map(false), world_map(true));
    swap.set_sources(&[
        ("main.luau", ENTER_MAIN),
        ("level1.tmj", &m1),
        ("level2.tmj", &m2),
    ])
    .unwrap();
    assert!(swap.not_carried().is_empty(), "{:?}", swap.not_carried());
    assert_eq!(shot(&mut never, 10), shot(&mut swap, 10));
    assert_eq!(ints(&mut swap, "probe"), vec![1, 1]);
}

#[test]
fn a_map_the_new_load_no_longer_has_starts_from_init_and_says_so() {
    let mut e = enter_engine(ENTER_MAIN, Some(""));
    run(&mut e, 0..10);
    let (m1, solo) = (
        world_map(false),
        ENTER_MAIN.replace("local level2 = world.load(\"level2\")\n", ""),
    );
    e.set_sources(&[("main.luau", &solo), ("level1.tmj", &m1)])
        .unwrap();
    let lines = e.not_carried();
    assert!(
        lines
            .iter()
            .any(|l| l == "world level1 was showing level2, which this load doesn't have"),
        "{lines:?}"
    );
    assert_eq!(ints(&mut e, "probe")[1], 0);
}

#[test]
fn a_program_cannot_switch_off_the_screen_off_check() {
    let main = ENTER_MAIN.replace(
        "force_blank = true w:enter",
        "__ppu_hot_swap = true w:enter",
    );
    let mut e = enter_engine(&main, Some(""));
    run(&mut e, 0..5);
    let err = e.frame(5.0 / 60.0, 5).unwrap_err().message;
    assert!(err.contains("force_blank"), "{err}");
}

#[test]
fn a_reordered_load_puts_each_world_back_on_its_own_map() {
    let mut e = enter_engine(ENTER_MAIN, Some(""));
    run(&mut e, 0..10);
    let (l1, l2) = (
        "local w = world.load(\"level1\")\n",
        "local level2 = world.load(\"level2\")\n",
    );
    let swapped = ENTER_MAIN.replace(&format!("{l1}{l2}"), &format!("{l2}{l1}"));
    assert_ne!(swapped, ENTER_MAIN);
    let (m1, m2) = (world_map(false), world_map(true));
    e.set_sources(&[
        ("main.luau", &swapped),
        ("level1.tmj", &m1),
        ("level2.tmj", &m2),
    ])
    .unwrap();
    assert!(e.not_carried().is_empty(), "{:?}", e.not_carried());
    assert_eq!(ints(&mut e, "probe"), vec![1, 1]);
}

/// A recompile during a faded switch: the fade goes on where it was and still lands the switch, as with no edit.
#[test]
fn a_recompile_mid_fade_keeps_the_fade_and_its_switch() {
    let a = |c: &str| format!("local function frame(t, f)\n  brightness = 15\n  if f == 2 then scenes.go(B, {{fade = 4}}) end\nend -- {c}\nreturn {{frame = frame}}\n");
    let files = |a: &str| vec![
        ("main.luau", "A = scenes.add(require(\"./a\"))\nB = scenes.add(require(\"./b\"))\n".to_string()),
        ("a.luau", a.to_string()),
        ("b.luau", "local function frame(t, f)\n  brightness = 9\nend\nreturn {frame = frame}\n".to_string()),
    ];
    let load = |e: &mut LuaEngine, a: &str| {
        let f = files(a);
        let refs: Vec<(&str, &str)> = f.iter().map(|(n, s)| (*n, s.as_str())).collect();
        e.set_sources(&refs).unwrap();
    };
    let level = |e: &mut LuaEngine, f: u32| e.frame(f as f64 / 60.0, f).unwrap().rows[100].brightness;
    let mut e = LuaEngine::new();
    load(&mut e, &a("one"));
    let mut seen: Vec<u8> = (0..4).map(|f| level(&mut e, f)).collect();
    load(&mut e, &a("two"));
    // The edit replays update 3, then the fade goes on.
    seen.extend((3..12).map(|f| level(&mut e, f)));
    assert_eq!(seen, vec![15, 15, 11, 7, 7, 3, 0, 0, 3, 7, 11, 15, 9]);
    assert_eq!(e.current_scene(), 1);
    // Review 2, Minor 3: an edit on the update the fade-out ends (5) keeps the pending switch to B.
    let mut e = LuaEngine::new();
    load(&mut e, &a("one"));
    let mut seen: Vec<u8> = (0..6).map(|f| level(&mut e, f)).collect();
    load(&mut e, &a("two"));
    seen.extend((5..12).map(|f| level(&mut e, f)));
    assert_eq!(seen, vec![15, 15, 11, 7, 3, 0, 0, 0, 3, 7, 11, 15, 9]);
    assert_eq!(e.current_scene(), 1);
}

/// A HUD field made in init() mirrors VRAM: the recompile's init draws it as 0 again, so the old field's
/// memory of what it showed must not carry over it, or `set` skips the redraw and the HUD stays 0 (PPU-1337).
#[test]
fn a_hud_field_made_in_init_shows_the_carried_value_after_a_swap() {
    let src = |c: &str| {
        format!("local hud = require(\"@ppu/hud\")\nlocal HUD = {{map = 0x7000, width = 32, height = 4, palette = 0}}\nlocal n = 0\nlocal count\nfunction init() count = hud.field(HUD, 0, 0, 1) end\nfunction frame(t, f) if f == 0 then n = 3 end count:set(n) end -- {c}")
    };
    let mut e = engine(&[("main.luau", &src("a"))]);
    run(&mut e, 0..2);
    assert_eq!(e.memory().vram[0x7000], 16 + 3);
    e.set_sources(&[("main.luau", &src("b"))]).unwrap();
    run(&mut e, 2..3);
    assert_eq!(e.memory().vram[0x7000], 16 + 3, "the carried 3, not init's redrawn 0");
}
