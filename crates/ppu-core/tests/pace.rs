//! The update rate: a project at 30 Hz (`targetHz` 25 or 30, or `---@snes frames 2`) runs frame() on every second
//! video frame only, as the cartridge does, with the update's own t and f; between updates the picture holds and the
//! sound plays on.
use ppu_core::LuaEngine;

const SEEN: &str = "seen = {}\nfunction frame(t, f)\n  seen[#seen + 1] = f\n  obj[0].x = f\n  obj[0].on = true\nend\nfunction fs() return seen end\n";

fn engine(files: &[(&str, &str)], hz: Option<u8>) -> LuaEngine {
    let mut e = LuaEngine::new();
    e.set_target_hz(hz).unwrap();
    e.set_sources(files).unwrap();
    e
}

/// Runs video frames `fs` (t = f / 60), each one's audio span rendered; the f values frame() saw.
fn play(e: &mut LuaEngine, fs: impl IntoIterator<Item = u32>) -> Vec<i64> {
    for f in fs {
        e.frame(f as f64 / 60.0, f)
            .unwrap_or_else(|err| panic!("frame {f}: {err:?}"));
        assert!(
            !e.audio().is_empty(),
            "video frame {f} renders its audio span"
        );
    }
    e.call_ints("fs", &[]).unwrap()
}

#[test]
fn a_30_hz_target_runs_frame_on_every_second_video_frame() {
    for hz in [30, 25] {
        let mut e = engine(&[("main.lua", SEEN)], Some(hz));
        assert_eq!(play(&mut e, 0..9), [0, 2, 4, 6, 8], "{hz} Hz");
    }
    let mut e = engine(&[("main.lua", SEEN)], Some(60));
    assert_eq!(play(&mut e, 0..4), [0, 1, 2, 3]);
}

#[test]
fn between_updates_the_picture_holds() {
    let mut e = engine(&[("main.lua", SEEN)], Some(30));
    e.frame(0.0, 0).unwrap();
    let a = e.frame(2.0 / 60.0, 2).unwrap();
    let b = e.frame(3.0 / 60.0, 3).unwrap();
    assert_eq!(a.rows, b.rows);
    assert_eq!(e.memory().oam[0].x, 2);
}

#[test]
fn the_annotation_sets_the_rate_and_the_build_setting_wins() {
    let annotated = format!("---@snes frames 2\n{SEEN}");
    let mut e = engine(&[("main.lua", &annotated)], None);
    assert_eq!(play(&mut e, 0..5), [0, 2, 4]);
    let mut e = engine(&[("main.lua", &annotated)], Some(60));
    assert_eq!(play(&mut e, 0..3), [0, 1, 2]);
    let mut e = engine(&[("main.lua", SEEN)], None);
    assert_eq!(play(&mut e, 0..3), [0, 1, 2]);
}

#[test]
fn a_jump_between_updates_runs_the_update_it_belongs_to() {
    let mut e = engine(&[("main.lua", "seen = {}\nts = {}\nfunction frame(t, f)\n  seen[#seen + 1] = f\n  ts[#ts + 1] = math.floor(t * 600 + 0.5)\nend\nfunction fs() return seen end\nfunction tts() return ts end\n")], Some(30));
    assert_eq!(play(&mut e, [0, 7, 8]), [0, 6, 8]);
    assert_eq!(
        e.call_ints("tts", &[]).unwrap(),
        [0, 60, 80],
        "t is the update's own"
    );
}

#[test]
fn an_edit_between_updates_reruns_that_update() {
    let mut e = engine(&[("main.lua", SEEN)], Some(30));
    play(&mut e, 0..4);
    e.set_sources(&[(
        "main.lua",
        &SEEN.replace("obj[0].x = f", "obj[0].x = f + 100"),
    )])
    .unwrap();
    e.frame(3.0 / 60.0, 3).unwrap();
    assert_eq!(e.memory().oam[0].x, 102);
}

#[test]
fn a_scene_counts_its_updates_from_its_entry() {
    let main = "A = scenes.add(require(\"./a\"))\nB = scenes.add(require(\"./b\"))\n";
    let a = "local function frame(t, f)\n  if f == 4 then scenes.go(B) end\nend\nreturn {frame = frame}\n";
    let b = "seen = {}\nlocal function frame(t, f)\n  seen[#seen + 1] = f\nend\nfunction fs() return seen end\nreturn {frame = frame}\n";
    let mut e = engine(
        &[("main.luau", main), ("a.luau", a), ("b.luau", b)],
        Some(30),
    );
    // a's update at f 4 asks for b, entered on the next update (video frame 6): b counts 0, 2, 4.
    assert_eq!(play(&mut e, 0..11), [0, 2, 4]);
}

#[test]
fn an_unknown_target_is_refused() {
    assert!(LuaEngine::new()
        .set_target_hz(Some(40))
        .unwrap_err()
        .contains("25, 30, 50 or 60"));
}

#[test]
fn a_poke_between_updates_shows_at_once() {
    let mut e = engine(&[("main.lua", SEEN)], Some(30));
    play(&mut e, 0..4);
    e.set_pokes("function __ppu_pokes()\n  obj[0].x = 77\nend\n")
        .unwrap();
    e.frame(3.0 / 60.0, 3).unwrap();
    assert_eq!(e.memory().oam[0].x, 77);
}
