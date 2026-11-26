//! `pad.pressed.<button>` and `pad.released.<button>`: the engine keeps the last update's buttons. The first
//! update of the program and of each scene has no edges, so a button held across scenes.go is not a press.
use ppu_core::LuaEngine;

const A: u16 = 1 << 4;

const MAIN: &str = r#"
scenes.add(require("./a"))
B = scenes.add(require("./b"))
log = {}
function probe() return log end
"#;

// Each scene logs, per update, held/pressed/released of A as 0/1 digits; the first leaves on update 3.
const SCENE: &str = r#"
local function frame(t, f)
  log[#log + 1] = (if pad.a then 100 else 0) + (if pad.pressed.a then 10 else 0) + (if pad.released.a then 1 else 0)
  if SWITCH and f == 3 then scenes.go(B) end
end
return {frame = frame}
"#;

#[test]
fn edges_follow_the_button_and_a_scene_starts_with_none() {
    let a = format!("SWITCH = true\n{SCENE}");
    let b = SCENE.replace("SWITCH and", "false and");
    let mut e = LuaEngine::new();
    e.set_sources(&[("main.lua", MAIN), ("a.lua", &a), ("b.lua", &b)])
        .unwrap();
    // A held from update 2 to 5 (the switch lands on update 4), released at 6.
    let pads = [0, 0, A, A, A, A, 0, 0];
    for (f, &p) in pads.iter().enumerate() {
        e.set_pad(p);
        e.frame(f as f64 / 60.0, f as u32).unwrap();
    }
    let log = e.call_ints("probe", &[]).unwrap();
    // 2: pressed; 3: held; 4: the game's first update, held but no edge; 5: held; 6: released.
    assert_eq!(log, vec![0, 0, 110, 100, 100, 100, 1, 0]);
}

#[test]
fn a_repeated_frame_sees_the_same_edges() {
    let mut e = LuaEngine::new();
    e.set_source("n = 0\nfunction frame(t, f) if pad.pressed.a then n = n + 1 end end\nfunction probe() return {n} end").unwrap();
    for (f, p) in [(0, 0), (1, A), (1, A), (2, A)] {
        e.set_pad(p);
        e.frame(f as f64 / 60.0, f).unwrap();
    }
    // Update 1's press counts on its first run and its paused repeat, not on update 2.
    assert_eq!(e.call_ints("probe", &[]).unwrap(), vec![2]);
}

#[test]
fn edges_follow_the_previous_update_at_30_hz() {
    let mut e = LuaEngine::new();
    e.set_target_hz(Some(30)).unwrap();
    e.set_source("log = {}\nfunction frame(t, f) log[#log + 1] = (if pad.pressed.a then 10 else 0) + (if pad.released.a then 1 else 0) end\nfunction probe() return log end").unwrap();
    // One update every second video frame: A goes down on the third update and up on the fifth.
    for (k, p) in [0, 0, A, A, 0, 0].into_iter().enumerate() {
        let f = 2 * k as u32;
        e.set_pad(p);
        e.frame(f as f64 / 60.0, f).unwrap();
    }
    assert_eq!(e.call_ints("probe", &[]).unwrap(), vec![0, 0, 10, 0, 1, 0]);
}
