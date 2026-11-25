//! Scenes: `scenes.add(require("./x"))` registers a module's {init, frame}, `scenes.go` switches
//! with a cold boot. The two-scene fixture (tests/fixtures/scenes-two, a project directory the
//! cartridge compiler's tests can load too) holds the switch to the game booted directly.
mod common;

use ppu_core::{convert_source, render_frame, ConvertOptions, LuaEngine, SourceKind};

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/scenes-two");
const A: u16 = 1 << 4;

/// An engine with the fixture's sources (ppu.json) and `files`, read from the fixture unless given.
fn fixture(files: &[(&str, Option<&str>)]) -> LuaEngine {
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(format!("{FIXTURE}/ppu.json")).unwrap())
            .unwrap();
    let mut e = LuaEngine::new();
    for s in manifest["sources"].as_array().unwrap() {
        let kind: SourceKind = s["kind"].as_str().unwrap().parse().unwrap();
        let opts: ConvertOptions = serde_json::from_value(s["options"].clone()).unwrap();
        let decoder = png::Decoder::new(
            std::fs::File::open(format!("{FIXTURE}/{}", s["file"].as_str().unwrap())).unwrap(),
        );
        let mut reader = decoder.read_info().unwrap();
        let mut rgba = vec![0u8; reader.output_buffer_size()];
        let info = reader.next_frame(&mut rgba).unwrap();
        assert_eq!(info.color_type, png::ColorType::Rgba);
        let (payload, _) = convert_source(
            kind,
            &opts,
            &rgba[..info.buffer_size()],
            info.width,
            info.height,
        )
        .unwrap();
        e.add_source(s["name"].as_str().unwrap(), &payload.encode())
            .unwrap();
    }
    let sources: Vec<(String, String)> = files
        .iter()
        .map(|(name, src)| {
            let src = src.map_or_else(
                || std::fs::read_to_string(format!("{FIXTURE}/{name}")).unwrap(),
                str::to_string,
            );
            (name.to_string(), src)
        })
        .collect();
    let refs: Vec<(&str, &str)> = sources
        .iter()
        .map(|(n, s)| (n.as_str(), s.as_str()))
        .collect();
    e.set_sources(&refs).unwrap();
    e
}

/// Update `f` at 60 Hz: the picture and the audio span.
fn step(e: &mut LuaEngine, f: u32) -> (Vec<u8>, Vec<i16>) {
    let lt = e
        .frame(f as f64 / 60.0, f)
        .unwrap_or_else(|err| panic!("frame {f}: {err:?}"));
    (render_frame(&lt, e.memory()), e.audio().to_vec())
}

fn ints(e: &mut LuaEngine, name: &str) -> Vec<i64> {
    e.call_ints(name, &[]).unwrap()
}

const PROBE: &str = "function probe() return {ticks} end";

#[test]
fn entering_the_game_matches_the_game_booted_directly() {
    let mut scenes = fixture(&[
        ("main.luau", None),
        ("title.luau", None),
        ("game.luau", None),
        ("probe.luau", Some(PROBE)),
    ]);
    let mut direct = fixture(&[
        ("main.luau", Some("GAME = scenes.add(require(\"./game\"))")),
        ("game.luau", None),
    ]);
    // The title plays its tone and runs its timer; A on update 10 enters the game on update 11.
    let mut title = vec![];
    for f in 0..=10 {
        scenes.set_pad(if f == 10 { A } else { 0 });
        title.push(step(&mut scenes, f));
    }
    scenes.set_pad(0);
    assert!(title[5].1.iter().any(|&s| s != 0), "the title sounds");
    let ticks = ints(&mut scenes, "probe")[0];
    assert!(ticks > 0, "the title's timer runs");
    for k in 0..60u32 {
        let (pixels, audio) = step(&mut scenes, 11 + k);
        let (want_pixels, want_audio) = step(&mut direct, k);
        assert!(
            pixels == want_pixels,
            "game update {k}: the picture differs from the game booted directly"
        );
        if k < 30 {
            assert!(
                audio.iter().all(|&s| s == 0),
                "game update {k}: silent until the game's own bell"
            );
        }
        assert_eq!(
            audio, want_audio,
            "game update {k}: the audio differs from the game booted directly"
        );
    }
    assert!(
        step(&mut direct, 60).1.iter().any(|&s| s != 0),
        "the bell rings"
    );
    assert_ne!(
        title[5].0,
        step(&mut direct, 61).0,
        "the title and the game look different"
    );
    assert_eq!(
        ints(&mut scenes, "probe")[0],
        ticks,
        "the title's timer stopped at the switch"
    );
}

const A_SCENE: &str = r#"
local function frame(t, f)
  if f == 4 then scenes.go(B) end
end
return {frame = frame}
"#;

const B_SCENE: &str = r#"
seen = {}
local function init()
  inits = (inits or 0) + 1
end
local function frame(t, f)
  seen[#seen + 1] = f
  seen[#seen + 1] = math.floor(t * 60 + 0.5)
end
return {init = init, frame = frame}
"#;

const MAIN: &str = "scenes.add(require(\"./a\"))\nB = scenes.add(require(\"./b\"))\nkept = 7\nfunction probe() return {inits or 0, kept, table.unpack(seen)} end";

fn ab() -> LuaEngine {
    let mut e = LuaEngine::new();
    e.set_sources(&[
        ("main.luau", MAIN),
        ("a.luau", A_SCENE),
        ("b.luau", B_SCENE),
    ])
    .unwrap();
    e
}

#[test]
fn the_entered_scene_counts_t_and_f_from_zero_and_globals_persist() {
    let mut e = ab();
    for f in 0..=7 {
        e.frame(f as f64 / 60.0, f).unwrap();
    }
    // go() on update 4 lands on update 5: the game sees f = 0, 1, 2 and t = 0, 1/60, 2/60.
    assert_eq!(ints(&mut e, "probe"), vec![1, 7, 0, 0, 1, 1, 2, 2]);
}

#[test]
fn a_paused_repeat_never_switches_and_a_jump_back_boots_the_first_scene() {
    let mut e = ab();
    for f in 0..=4 {
        e.frame(f as f64 / 60.0, f).unwrap();
    }
    // A paused refresh of update 4 runs its frame() again but does not switch.
    e.frame(4.0 / 60.0, 4).unwrap();
    assert_eq!(ints(&mut e, "probe"), vec![0, 7]);
    e.frame(5.0 / 60.0, 5).unwrap();
    e.frame(6.0 / 60.0, 6).unwrap();
    assert_eq!(ints(&mut e, "probe"), vec![1, 7, 0, 0, 1, 1]);
    // Scrubbing back before the game's entry boots the first scene; its go() enters the game again.
    e.frame(2.0 / 60.0, 2).unwrap();
    e.frame(3.0 / 60.0, 3).unwrap();
    e.frame(4.0 / 60.0, 4).unwrap();
    e.frame(5.0 / 60.0, 5).unwrap();
    assert_eq!(ints(&mut e, "probe"), vec![2, 7, 0, 0, 1, 1, 0, 0]);
}

#[test]
fn each_scene_places_its_own_memory() {
    let mut e = fixture(&[
        ("main.luau", None),
        ("title.luau", None),
        ("game.luau", None),
    ]);
    // Both scenes place at char 0x0000: the title's picture shows until the switch, the game's after.
    e.frame(0.0, 0).unwrap();
    let shown = e.memory().vram[..0x400].to_vec();
    e.set_pad(A);
    e.frame(1.0 / 60.0, 1).unwrap();
    e.set_pad(0);
    e.frame(2.0 / 60.0, 2).unwrap();
    assert_ne!(
        e.memory().vram[..0x400],
        shown[..],
        "the game's picture replaced the title's"
    );
    assert!(
        e.memory().vram[0x7000] == 0 && e.memory().cgram[200] == 0,
        "the title's pokes are gone"
    );
    // Inside one scene, setup placements still may not overlap.
    let mut e = LuaEngine::new();
    common::add_sheet(&mut e, "tiles", vec![255; 16 * 8 * 4], 16, 8, 4);
    let one = "dma('tiles', {char = 0x1000, pal = 0})\ndma('tiles', {char = 0x1000, pal = 16})\nreturn {frame = function() end}";
    let err = e
        .set_sources(&[
            ("main.luau", "scenes.add(require(\"./one\"))"),
            ("one.luau", one),
        ])
        .unwrap_err();
    assert!(err.message.contains("overlaps VRAM"), "{err:?}");
}

#[test]
fn a_program_of_scenes_has_no_global_frame() {
    let mut e = LuaEngine::new();
    let err = e
        .set_sources(&[
            (
                "main.luau",
                "scenes.add(require(\"./a\"))\nfunction frame(t, f) end",
            ),
            ("a.luau", A_SCENE),
        ])
        .unwrap_err();
    assert!(err.message.contains("each scene's own frame()"), "{err:?}");
    let err = e
        .set_sources(&[
            ("main.luau", "local a = require(\"./a\")\nscenes.add(a)"),
            ("a.luau", A_SCENE),
        ])
        .unwrap_err();
    assert!(err.message.contains("scenes.add(require"), "{err:?}");
}

#[test]
fn a_scene_that_leaves_the_sprite_setup_to_its_groups_draws_the_same() {
    let game = std::fs::read_to_string(format!("{FIXTURE}/game.luau")).unwrap();
    let bare = game
        .replace("  obj.char_base = hero.char\n", "")
        .replace("  screen.main.obj = true\n", "");
    assert_ne!(bare, game);
    // The title writes screen.main.obj for itself; it draws no group, so the game's setup is still the library's.
    let mut set = fixture(&[
        ("main.luau", None),
        ("title.luau", None),
        ("game.luau", None),
    ]);
    let mut left = fixture(&[
        ("main.luau", None),
        ("title.luau", None),
        ("game.luau", Some(&bare)),
    ]);
    for f in 0..=14 {
        for e in [&mut set, &mut left] {
            e.set_pad(if f == 10 { A } else { 0 });
        }
        assert_eq!(step(&mut left, f).0, step(&mut set, f).0, "update {f}");
    }
    assert_eq!(left.memory().obsel.char_base, 0x6000);
}

#[test]
fn the_switcher_enters_the_game_as_the_game_booted_directly() {
    let mut chosen = fixture(&[
        ("main.luau", None),
        ("title.luau", None),
        ("game.luau", None),
    ]);
    let mut direct = fixture(&[
        ("main.luau", Some("GAME = scenes.add(require(\"./game\"))")),
        ("game.luau", None),
    ]);
    assert_eq!(chosen.scenes(), ["title.luau", "game.luau"]);
    assert_eq!(chosen.current_scene(), 0);
    chosen.set_start_scene(Some("game.luau".into()), true);
    chosen.reset().unwrap();
    assert_eq!(chosen.current_scene(), 1);
    for f in 0..61 {
        let (pixels, audio) = step(&mut chosen, f);
        let (want_pixels, want_audio) = step(&mut direct, f);
        assert!(
            pixels == want_pixels,
            "update {f}: the picture differs from the game booted directly"
        );
        assert_eq!(
            audio, want_audio,
            "update {f}: the audio differs from the game booted directly"
        );
    }
}

const FIRST: &str = "local function init() a_inits = (a_inits or 0) + 1 end\nlocal function frame(t, f) a_frames = (a_frames or 0) + 1 end\nreturn {init = init, frame = frame}";
const SECOND: &str = "local function init() b_inits = (b_inits or 0) + 1 end\nlocal function frame(t, f) b_seen = f if f == 2 then scenes.go(A) end end\nreturn {init = init, frame = frame}";
const PAIR: &str = "A = scenes.add(require(\"./first\"))\nB = scenes.add(require(\"./second\"))\nkept = 7\nfunction probe() return {a_inits or 0, a_frames or 0, b_inits or 0, b_seen or -1, kept, B} end";

#[test]
fn a_chosen_scene_boots_after_file_level_and_skips_earlier_scenes() {
    let mut e = LuaEngine::new();
    e.set_start_scene(Some("second.luau".into()), true);
    e.set_sources(&[
        ("main.luau", PAIR),
        ("first.luau", FIRST),
        ("second.luau", SECOND),
    ])
    .unwrap();
    assert_eq!(e.scenes(), ["first.luau", "second.luau"]);
    assert_eq!(e.current_scene(), 1);
    e.frame(0.0, 0).unwrap();
    // File level ran (kept, the B handle), the chosen scene's init once, the first scene never.
    assert_eq!(ints(&mut e, "probe"), vec![0, 0, 1, 0, 7, 2]);
    for f in 1..=3 {
        e.frame(f as f64 / 60.0, f).unwrap();
    }
    // Its go() on update 2 lands on update 3: the running scene follows.
    assert_eq!(e.current_scene(), 0);
    assert_eq!(ints(&mut e, "probe"), vec![1, 1, 1, 2, 7, 2]);
    // A jump back before the entry boots the chosen scene again, not the first.
    e.frame(1.0 / 60.0, 1).unwrap();
    assert_eq!(e.current_scene(), 1);
    // Clearing the choice boots as the program does at the next load.
    e.set_start_scene(None, true);
    e.reset().unwrap();
    assert_eq!(e.current_scene(), 0);
    // A file that is no scene of the program leaves the normal boot.
    e.set_start_scene(Some("gone.luau".into()), true);
    e.reset().unwrap();
    assert_eq!(e.current_scene(), 0);
}

#[test]
fn a_toy_without_scenes_is_one_implied_scene() {
    let mut e = LuaEngine::new();
    e.set_sources(&[
        ("util.luau", "x = 1"),
        ("main.luau", "function frame(t, f) end"),
    ])
    .unwrap();
    assert_eq!(e.scenes(), ["main.luau"]);
    assert_eq!(e.current_scene(), 0);
    e.set_start_scene(Some("main.luau".into()), true);
    e.reset().unwrap();
    e.frame(0.0, 0).unwrap();
    assert_eq!(e.current_scene(), 0);
}

// Each step appends a code to `log`: file level 1, 2 (modules) and 3 (main, after every add);
// a scene's preview 10/20, init 11/21 (plus State.hp, so 31 = staged, 33 = as main set it), frame
// at f = 0 12/22. The first scene goes to the second on update 1.
const STAGED_MAIN: &str = "log = log or {}\nState = {hp = 3}\nA = scenes.add(require(\"./first\"))\nB = scenes.add(require(\"./second\"))\ntable.insert(log, 3)\nfunction probe() return log end";
const STAGED_FIRST: &str = "log = log or {}\ntable.insert(log, 1)\nlocal function preview() table.insert(log, 10) end\nlocal function init() table.insert(log, 11) end\nlocal function frame(t, f) if f == 0 then table.insert(log, 12) end if f == 1 then scenes.go(B) end end\nreturn {init = init, frame = frame, preview = preview}";
const STAGED_SECOND: &str = "table.insert(log, 2)\nlocal function preview() State.hp = 1 table.insert(log, 20) end\nlocal function init() table.insert(log, 21) table.insert(log, State.hp + 30) end\nlocal function frame(t, f) if f == 0 then table.insert(log, 22) end end\nreturn {init = init, frame = frame, preview = preview}";

fn staged(start: Option<&str>) -> LuaEngine {
    let mut e = LuaEngine::new();
    e.set_start_scene(start.map(str::to_string), true);
    e.set_sources(&[
        ("main.luau", STAGED_MAIN),
        ("first.luau", STAGED_FIRST),
        ("second.luau", STAGED_SECOND),
    ])
    .unwrap();
    e
}

#[test]
fn a_scene_view_entry_runs_the_scenes_preview_after_file_level_and_before_init() {
    let mut e = staged(Some("second.luau"));
    e.frame(0.0, 0).unwrap();
    // File level, every scenes.add, the preview (its State.hp = 1 seen by init), init, frame.
    assert_eq!(ints(&mut e, "probe"), vec![1, 2, 3, 20, 21, 31, 22]);
    // A reload into the chosen scene (an edit, Restart) is another Scene view entry.
    e.reset().unwrap();
    e.frame(0.0, 0).unwrap();
    assert_eq!(ints(&mut e, "probe"), vec![1, 2, 3, 20, 21, 31, 22]);
    // So is choosing the first scene: preview runs for it as well.
    let mut first = staged(Some("first.luau"));
    first.frame(0.0, 0).unwrap();
    assert_eq!(ints(&mut first, "probe"), vec![1, 2, 3, 10, 11, 12]);
}

#[test]
fn power_on_and_scenes_go_never_run_a_preview() {
    let mut e = staged(None);
    for f in 0..=2 {
        e.frame(f as f64 / 60.0, f).unwrap();
    }
    // The first scene from power-on, then its go() lands on update 2: init sees main's State.hp.
    assert_eq!(e.current_scene(), 1);
    assert_eq!(ints(&mut e, "probe"), vec![1, 2, 3, 11, 12, 21, 33, 22]);
    // Chosen first scene: its own entry previews, the scenes.go into the second does not.
    let mut chosen = staged(Some("first.luau"));
    for f in 0..=2 {
        chosen.frame(f as f64 / 60.0, f).unwrap();
    }
    assert_eq!(
        ints(&mut chosen, "probe"),
        vec![1, 2, 3, 10, 11, 12, 21, 33, 22]
    );
}

#[test]
fn a_preview_error_reports_like_an_init_error_naming_the_scene() {
    let mut e = LuaEngine::new();
    e.set_start_scene(Some("second.luau".into()), true);
    let second = "local function preview() error(\"no boss\") end\nlocal function frame(t, f) end\nreturn {frame = frame, preview = preview}";
    let err = e
        .set_sources(&[
            ("main.luau", STAGED_MAIN),
            ("first.luau", STAGED_FIRST),
            ("second.luau", second),
        ])
        .unwrap_err();
    assert_eq!(err.file.as_deref(), Some("second.luau"));
    assert!(err.message.contains("no boss"), "{}", err.message);
    // A preview that is no function is refused at scenes.add.
    let bad = "local function frame(t, f) end\nreturn {frame = frame, preview = 1}";
    let err = LuaEngine::new()
        .set_sources(&[
            ("main.luau", STAGED_MAIN),
            ("first.luau", STAGED_FIRST),
            ("second.luau", bad),
        ])
        .unwrap_err();
    assert!(
        err.message.contains("second.luau must return"),
        "{}",
        err.message
    );
}

#[test]
fn a_held_switch_enters_the_next_scene_at_t_zero_without_its_preview() {
    // Scene view holds a scenes.go: the first scene asks for the second on update 1.
    let mut e = staged(Some("first.luau"));
    assert_eq!(e.pending_scene(), None);
    e.frame(0.0, 0).unwrap();
    e.frame(1.0 / 60.0, 1).unwrap();
    assert_eq!(e.pending_scene(), Some(1));
    assert_eq!(e.current_scene(), 0, "held: not entered yet");
    // Entering it now starts its clock at t = 0: the next update is its f = 0, with no preview.
    assert!(e.enter_pending_scene().unwrap());
    assert_eq!(e.pending_scene(), None);
    assert_eq!(e.current_scene(), 1);
    e.frame(0.0, 0).unwrap();
    assert_eq!(ints(&mut e, "probe"), vec![1, 2, 3, 10, 11, 12, 21, 33, 22]);
    assert!(!e.enter_pending_scene().unwrap(), "nothing waits");
}

#[test]
fn a_reload_held_where_scenes_go_landed_never_previews() {
    // Power-on, the first scene goes to the second; live pokes there hold it for the next load.
    let mut e = staged(None);
    for f in 0..=2 {
        e.frame(f as f64 / 60.0, f).unwrap();
    }
    e.set_start_scene(Some("second.luau".into()), false);
    e.reset().unwrap();
    e.frame(0.0, 0).unwrap();
    assert_eq!(e.current_scene(), 1, "the reload lands in the second scene");
    // File level and init again, but no preview: main's State.hp reaches init (33, not 31).
    assert_eq!(ints(&mut e, "probe"), vec![1, 2, 3, 21, 33, 22]);
}

/// A score{} set up stopped (`at = false`) opens a silent master volume at load, as one set up playing does; the
/// cartridge opens it again at every scene's entry, so the preview's power cycle must too.
#[test]
fn a_stopped_score_reopens_the_master_volume_at_every_entry() {
    use ppu_core::song::{Pattern, Row, Song};
    let song = Song {
        tempo: 12000,
        swing: 0,
        key: 0,
        voice_mask: 0xff,
        rows: vec![Row {
            sound: "kick".into(),
            note: None,
            vol: 100,
            pan: 0,
            keep: false,
        }],
        patterns: vec![Pattern {
            name: "A".into(),
            length: 12,
            tempo: None,
            notes: vec![],
        }],
        arrangement: vec![0],
        loop_start: 0,
    };
    let mut e = LuaEngine::new();
    common::add_song_source(&mut e, "tune", &song);
    let main = format!("tune = score{{ song = \"tune\", at = false }}\n{MAIN}");
    e.set_sources(&[
        ("main.luau", main.as_str()),
        ("a.luau", A_SCENE),
        ("b.luau", B_SCENE),
    ])
    .unwrap();
    for f in 0..=7 {
        e.frame(f as f64 / 60.0, f).unwrap();
        assert_eq!(
            (e.dsp().read(0x0c), e.dsp().read(0x1c)),
            (0x7f, 0x7f),
            "update {f}"
        );
    }
    assert_eq!(ints(&mut e, "probe")[0], 1, "the switch entered b");
}

const RANDOM_MAIN: &str = "scenes.add(require(\"./roll\"))\nfunction probe() return draws end";
const RANDOM_SCENE: &str = "draws = {}\nlocal function preview() table.insert(draws, math.random(1, 1000)) end\nlocal function frame(t, f) table.insert(draws, math.random(1, 1000)) end\nreturn {frame = frame, preview = preview}";

#[test]
fn every_start_point_reseeds_math_random_so_a_replay_draws_the_same_numbers() {
    let run = |e: &mut LuaEngine| {
        e.reset().unwrap();
        for f in 0..4 {
            e.frame(f as f64 / 60.0, f).unwrap();
        }
        ints(e, "probe")
    };
    let mut e = LuaEngine::new();
    e.set_sources(&[("main.luau", RANDOM_MAIN), ("roll.luau", RANDOM_SCENE)])
        .unwrap();
    // Power-on twice: the generator state left by the first run never reaches the second.
    let first = run(&mut e);
    assert_eq!(first.len(), 4);
    assert_eq!(run(&mut e), first);
    // A Scene view entry (its preview draws first) twice: the same draws again.
    e.set_start_scene(Some("roll.luau".into()), true);
    let entry = run(&mut e);
    assert_eq!(entry.len(), 5);
    assert_eq!(run(&mut e), entry);
    // Reseeded, not merely repeated: the entry draws the power-on sequence from its first number.
    assert_eq!(entry[..4], first[..]);
}

/// `scenes.go(B, {fade = 4})` on update 2: A keeps running while the screen darkens (12, 7, 3, 0 on updates 2-5,
/// A's own `brightness = 15` overruled), B enters on update 6 and brightens (0, 3, 7, 11, 15), then B's own writes
/// show. A plain go asked mid-fade changes nothing; a paused repeat shows its step again without taking another; a
/// jump back before B boots A with no fade left.
#[test]
fn a_faded_switch_darkens_the_old_scene_and_brightens_the_new() {
    let mut e = LuaEngine::new();
    e.set_sources(&[
        ("main.luau", "A = scenes.add(require(\"./a\"))\nB = scenes.add(require(\"./b\"))\n"),
        ("a.luau", "local function frame(t, f)\n  brightness = 15\n  if f == 2 then scenes.go(B, {fade = 4}) end\n  if f == 3 then scenes.go(A) end\nend\nreturn {frame = frame}\n"),
        ("b.luau", "local function frame(t, f)\n  brightness = 9\nend\nreturn {frame = frame}\n"),
    ])
    .unwrap();
    let level = |e: &mut LuaEngine, f: u32| e.frame(f as f64 / 60.0, f).unwrap().rows[100].brightness;
    let seen: Vec<u8> = (0..13).map(|f| level(&mut e, f)).collect();
    assert_eq!(seen, vec![15, 15, 11, 7, 3, 0, 0, 3, 7, 11, 15, 9, 9]);
    assert_eq!(e.current_scene(), 1);
    // A paused repeat mid-fade: the same step again, and the fade goes on from there.
    let mut e2 = LuaEngine::new();
    e2.set_sources(&[
        ("main.luau", "A = scenes.add(require(\"./a\"))\nB = scenes.add(require(\"./b\"))\n"),
        ("a.luau", "local function frame(t, f)\n  if f == 2 then scenes.go(B, {fade = 4}) end\nend\nreturn {frame = frame}\n"),
        ("b.luau", "local function frame(t, f) end\nreturn {frame = frame}\n"),
    ])
    .unwrap();
    let mut seen = vec![];
    for f in [0, 1, 2, 3, 3, 4, 5, 6] {
        seen.push(level(&mut e2, f));
    }
    assert_eq!(seen, vec![15, 15, 11, 7, 7, 3, 0, 0]);
    // Back before B's entry: A boots unfaded.
    assert_eq!(level(&mut e2, 1), 15);
    assert_eq!(e2.current_scene(), 0);
}

#[test]
fn a_fade_is_a_whole_number_of_frames_asked_while_the_game_runs() {
    for (go, file_level) in [("scenes.go(B, {fade = 0})", false), ("scenes.go(B, {fade = 2.5})", false), ("scenes.go(B, {fade = 4, x = 1})", false), ("scenes.go(B, {fade = 4})", true)] {
        let mut e = LuaEngine::new();
        let main = format!("A = scenes.add(require(\"./a\"))\nB = scenes.add(require(\"./b\"))\n{}", if file_level { go } else { "" });
        let a = format!("local function frame(t, f)\n  {}\nend\nreturn {{frame = frame}}\n", if file_level { "" } else { go });
        let loaded = e.set_sources(&[("main.luau", main.as_str()), ("a.luau", a.as_str()), ("b.luau", "local function frame(t, f) end\nreturn {frame = frame}\n")]);
        let failed = loaded.err().or_else(|| e.frame(0.0, 0).err()).unwrap_or_else(|| panic!("{go} must fail"));
        assert!(failed.message.contains("scenes.go"), "{go}: {}", failed.message);
    }
}

/// The Studio lands a switch itself: after the update that asks for it, `enter_pending_scene` enters it with the clock
/// starting over (transport.ts follow/enterHeld). A faded switch still brightens the entered scene, as on the cart.
#[test]
fn a_faded_switch_the_studio_enters_still_fades_in() {
    let mut e = LuaEngine::new();
    e.set_sources(&[
        ("main.luau", "A = scenes.add(require(\"./a\"))\nB = scenes.add(require(\"./b\"))\n"),
        ("a.luau", "local function frame(t, f)\n  brightness = 15\n  if f == 2 then scenes.go(B, {fade = 4}) end\nend\nreturn {frame = frame}\n"),
        ("b.luau", "local function frame(t, f)\n  brightness = 9\nend\nreturn {frame = frame}\n"),
    ])
    .unwrap();
    let (mut seen, mut f) = (vec![], 0);
    for _ in 0..12 {
        seen.push(e.frame(f as f64 / 60.0, f).unwrap().rows[100].brightness);
        f += 1;
        if e.pending_scene().is_some() {
            assert!(e.enter_pending_scene().unwrap());
            f = 0;
        }
    }
    assert_eq!(seen, vec![15, 15, 11, 7, 3, 0, 0, 3, 7, 11, 15, 9]);
    assert_eq!(e.current_scene(), 1);
}

/// `{fade = n}` counts video frames: at 30 Hz an update spans two, so the fade takes two steps a update and lasts
/// as long in real time as at 60 Hz (the cart's dispatcher steps by the frames f advanced).
#[test]
fn a_fade_counts_video_frames_at_30_hz() {
    let mut e = LuaEngine::new();
    e.set_target_hz(Some(30)).unwrap();
    e.set_sources(&[
        ("main.luau", "A = scenes.add(require(\"./a\"))\nB = scenes.add(require(\"./b\"))\n"),
        ("a.luau", "local function frame(t, f)\n  brightness = 15\n  if f == 2 then scenes.go(B, {fade = 4}) end\nend\nreturn {frame = frame}\n"),
        ("b.luau", "local function frame(t, f)\n  brightness = 9\nend\nreturn {frame = frame}\n"),
    ])
    .unwrap();
    let seen: Vec<u8> = (0..14).map(|f| e.frame(f as f64 / 60.0, f).unwrap().rows[100].brightness).collect();
    assert_eq!(seen, vec![15, 15, 7, 7, 0, 0, 0, 0, 7, 7, 15, 15, 9, 9]);
}

/// An odd fade at 30 Hz steps past its length: the last step still shows full brightness, so a scene that never
/// writes brightness is not left dim (review 2, Major 2: fade 5 stuck at 12).
#[test]
fn an_odd_fade_at_30_hz_ends_at_full_brightness() {
    let mut e = LuaEngine::new();
    e.set_target_hz(Some(30)).unwrap();
    e.set_sources(&[
        ("main.luau", "A = scenes.add(require(\"./a\"))\nB = scenes.add(require(\"./b\"))\n"),
        ("a.luau", "local function frame(t, f)\n  if f == 0 then scenes.go(B, {fade = 5}) end\nend\nreturn {frame = frame}\n"),
        ("b.luau", "local function frame(t, f)\nend\nreturn {frame = frame}\n"),
    ])
    .unwrap();
    let seen: Vec<u8> = (0..20).map(|f| e.frame(f as f64 / 60.0, f).unwrap().rows[100].brightness).collect();
    assert_eq!(seen[16..], [15, 15, 15, 15], "{seen:?}");
}
