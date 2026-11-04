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
        serde_json::from_str(&std::fs::read_to_string(format!("{FIXTURE}/ppu.json")).unwrap()).unwrap();
    let mut e = LuaEngine::new();
    for s in manifest["sources"].as_array().unwrap() {
        let kind: SourceKind = s["kind"].as_str().unwrap().parse().unwrap();
        let opts: ConvertOptions = serde_json::from_value(s["options"].clone()).unwrap();
        let decoder = png::Decoder::new(std::fs::File::open(format!("{FIXTURE}/{}", s["file"].as_str().unwrap())).unwrap());
        let mut reader = decoder.read_info().unwrap();
        let mut rgba = vec![0u8; reader.output_buffer_size()];
        let info = reader.next_frame(&mut rgba).unwrap();
        assert_eq!(info.color_type, png::ColorType::Rgba);
        let (payload, _) = convert_source(kind, &opts, &rgba[..info.buffer_size()], info.width, info.height).unwrap();
        e.add_source(s["name"].as_str().unwrap(), &payload.encode()).unwrap();
    }
    let sources: Vec<(String, String)> = files
        .iter()
        .map(|(name, src)| {
            let src = src.map_or_else(|| std::fs::read_to_string(format!("{FIXTURE}/{name}")).unwrap(), str::to_string);
            (name.to_string(), src)
        })
        .collect();
    let refs: Vec<(&str, &str)> = sources.iter().map(|(n, s)| (n.as_str(), s.as_str())).collect();
    e.set_sources(&refs).unwrap();
    e
}

/// Update `f` at 60 Hz: the picture and the audio span.
fn step(e: &mut LuaEngine, f: u32) -> (Vec<u8>, Vec<i16>) {
    let lt = e.frame(f as f64 / 60.0, f).unwrap_or_else(|err| panic!("frame {f}: {err:?}"));
    (render_frame(&lt, e.memory()), e.audio().to_vec())
}

fn ints(e: &mut LuaEngine, name: &str) -> Vec<i64> {
    e.call_ints(name, &[]).unwrap()
}

const PROBE: &str = "function probe() return {ticks} end";

#[test]
fn entering_the_game_matches_the_game_booted_directly() {
    let mut scenes = fixture(&[("main.luau", None), ("title.luau", None), ("game.luau", None), ("probe.luau", Some(PROBE))]);
    let mut direct = fixture(&[("main.luau", Some("GAME = scenes.add(require(\"./game\"))")), ("game.luau", None)]);
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
        assert!(pixels == want_pixels, "game update {k}: the picture differs from the game booted directly");
        if k < 30 {
            assert!(audio.iter().all(|&s| s == 0), "game update {k}: silent until the game's own bell");
        }
        assert_eq!(audio, want_audio, "game update {k}: the audio differs from the game booted directly");
    }
    assert!(step(&mut direct, 60).1.iter().any(|&s| s != 0), "the bell rings");
    assert_ne!(title[5].0, step(&mut direct, 61).0, "the title and the game look different");
    assert_eq!(ints(&mut scenes, "probe")[0], ticks, "the title's timer stopped at the switch");
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
    e.set_sources(&[("main.luau", MAIN), ("a.luau", A_SCENE), ("b.luau", B_SCENE)]).unwrap();
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
    let mut e = fixture(&[("main.luau", None), ("title.luau", None), ("game.luau", None)]);
    // Both scenes place at char 0x0000: the title's picture shows until the switch, the game's after.
    e.frame(0.0, 0).unwrap();
    let shown = e.memory().vram[..0x400].to_vec();
    e.set_pad(A);
    e.frame(1.0 / 60.0, 1).unwrap();
    e.set_pad(0);
    e.frame(2.0 / 60.0, 2).unwrap();
    assert_ne!(e.memory().vram[..0x400], shown[..], "the game's picture replaced the title's");
    assert!(e.memory().vram[0x7000] == 0 && e.memory().cgram[200] == 0, "the title's pokes are gone");
    // Inside one scene, setup placements still may not overlap.
    let mut e = LuaEngine::new();
    common::add_sheet(&mut e, "tiles", vec![255; 16 * 8 * 4], 16, 8, 4);
    let one = "dma('tiles', {char = 0x1000, pal = 0})\ndma('tiles', {char = 0x1000, pal = 16})\nreturn {frame = function() end}";
    let err = e.set_sources(&[("main.luau", "scenes.add(require(\"./one\"))"), ("one.luau", one)]).unwrap_err();
    assert!(err.message.contains("overlaps VRAM"), "{err:?}");
}

#[test]
fn a_program_of_scenes_has_no_global_frame() {
    let mut e = LuaEngine::new();
    let err = e
        .set_sources(&[("main.luau", "scenes.add(require(\"./a\"))\nfunction frame(t, f) end"), ("a.luau", A_SCENE)])
        .unwrap_err();
    assert!(err.message.contains("each scene's own frame()"), "{err:?}");
    let err = e.set_sources(&[("main.luau", "local a = require(\"./a\")\nscenes.add(a)"), ("a.luau", A_SCENE)]).unwrap_err();
    assert!(err.message.contains("scenes.add(require"), "{err:?}");
}
