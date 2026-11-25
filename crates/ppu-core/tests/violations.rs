//! A store that breaks its makeBounded or makeFixed contract never stops the preview: the engine
//! records it (store, value, bounds, file:line, scene, frame) and keeps the value as written.
use ppu_core::LuaEngine;

const MAIN: &str = "scenes.add(require(\"./title\"))\nGAME = scenes.add(require(\"./game\"))";
const TITLE: &str =
    "local function frame(t, f)\n  if f == 2 then scenes.go(GAME) end\nend\nreturn {frame = frame}";
const GAME: &str = "local s = {hp = makeBounded(3, 0, 3)}\nlocal speed = makeFixed(0, -2, 2, 16)\nlocal function frame(t, f)\n  s.hp -= 1\n  speed = speed + 0.75\n  obj[0].x = s.hp + 10\nend\nreturn {frame = frame}";

fn engine() -> LuaEngine {
    let mut e = LuaEngine::new();
    e.set_sources(&[
        ("main.luau", MAIN),
        ("title.luau", TITLE),
        ("game.luau", GAME),
    ])
    .unwrap();
    e
}

#[test]
fn violations_are_recorded_and_the_program_keeps_running() {
    let mut e = engine();
    for f in 0..10 {
        e.frame(f as f64 / 60., f).unwrap();
    }
    // The game is entered on update 3: hp 2, 1, 0, then -1 at frame 6; speed passes 2 at frame 5.
    let got: Vec<_> = e
        .violations()
        .iter()
        .map(|v| {
            (
                v.store.as_str(),
                v.value,
                v.min,
                v.max,
                v.file.as_str(),
                v.line,
                v.scene.as_str(),
                v.frame,
                v.count,
            )
        })
        .collect();
    assert_eq!(
        got,
        [
            (
                "speed",
                2.25,
                -2,
                2,
                "game.luau",
                5,
                "game.luau",
                Some(5),
                5
            ),
            ("hp", -1., 0, 3, "game.luau", 4, "game.luau", Some(6), 4),
        ]
    );
    // The value stays as written: no clamping.
    assert_eq!(e.memory().oam[0].x, 10 - 4);
    // A hot reload keeps the run's violations; a reset (a start point) clears them.
    e.set_sources(&[
        ("main.luau", MAIN),
        ("title.luau", TITLE),
        ("game.luau", GAME),
    ])
    .unwrap();
    assert_eq!(e.violations().len(), 2);
    e.reset().unwrap();
    assert!(e.violations().is_empty());
}

#[test]
fn at_30_hz_a_violation_carries_the_video_frame_of_its_update() {
    let mut e = LuaEngine::new();
    e.set_target_hz(Some(30)).unwrap();
    e.set_sources(&[(
        "main.luau",
        "local n = makeBounded(0, 0, 1)\nfunction frame(t, f)\n  n += 1\nend",
    )])
    .unwrap();
    for f in 0..6 {
        e.frame(f as f64 / 60., f).unwrap();
    }
    let v = &e.violations()[0];
    assert_eq!(
        (v.frame, v.count, v.scene.as_str()),
        (Some(2), 2, "main.luau")
    );
}

#[test]
fn a_store_no_cartridge_can_hold_still_stops() {
    let mut e = LuaEngine::new();
    e.set_sources(&[(
        "main.luau",
        "local s = {hp = makeBounded(3, 0, 3)}\nfunction frame()\n  s.hp = nil\nend",
    )])
    .unwrap();
    let err = e.frame(0., 0).unwrap_err();
    assert!(err.message.contains("requires a number"), "{err:?}");
    assert_eq!(
        (err.file.as_deref(), err.line),
        (Some("main.luau"), Some(3))
    );
    assert!(e.violations().is_empty());
}
