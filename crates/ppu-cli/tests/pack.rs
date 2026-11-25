use std::path::Path;

const SAMPLE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sample");
const SAMPLE_PPU_JSON: &str = include_str!("fixtures/sample.ppu.json");

fn run(args: &[&str]) -> std::process::Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_ppu"))
        .args(args)
        .output()
        .unwrap()
}

fn success(args: &[&str]) -> String {
    let output = run(args);
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn pack_sample_matches_committed_pin() {
    let packed = ppu_cli::pack(Path::new(SAMPLE_DIR)).unwrap();
    assert_eq!(packed.as_bytes(), SAMPLE_PPU_JSON.as_bytes());
}

#[test]
fn unpack_then_pack_round_trips_byte_for_byte() {
    let dir = tempfile::tempdir().unwrap();
    ppu_cli::unpack(SAMPLE_PPU_JSON, dir.path()).unwrap();

    assert!(dir.path().join("ppu.json").is_file());
    assert!(dir.path().join("main.lua").is_file());
    assert!(dir.path().join("pokes.lua").is_file());
    let payload = std::fs::read(dir.path().join(".ppu/sources/0.bin")).unwrap();
    assert_eq!(payload, vec![1, 2, 3, 4, 5]);

    let repacked = ppu_cli::pack(dir.path()).unwrap();
    assert_eq!(repacked.as_bytes(), SAMPLE_PPU_JSON.as_bytes());
}

#[test]
fn unpack_writes_the_luau_declarations_beside_a_luau_toy_and_pack_leaves_them_out() {
    let text = SAMPLE_PPU_JSON.replace("\"main.lua\"", "\"main.luau\"");
    let dir = tempfile::tempdir().unwrap();
    ppu_cli::unpack(&text, dir.path()).unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.path().join("ppu.d.luau")).unwrap(),
        ppu_core::LUAU_DECLARATIONS
    );
    for editor_file in [
        ".luaurc",
        ".vscode/settings.json",
        ".vscode/extensions.json",
        ".ppu/lib/hud.luau",
    ] {
        assert!(
            dir.path().join(editor_file).is_file(),
            "unpack writes {editor_file} like ppu new"
        );
    }
    assert_eq!(ppu_cli::pack(dir.path()).unwrap(), text);

    let lua = tempfile::tempdir().unwrap();
    ppu_cli::unpack(SAMPLE_PPU_JSON, lua.path()).unwrap();
    assert!(
        !lua.path().join("ppu.d.luau").exists(),
        "a Lua toy gets no Luau declarations"
    );
    assert!(
        !lua.path().join(".luaurc").exists(),
        "a Lua toy gets no luau-lsp setup"
    );
}

#[test]
fn pack_rejects_the_declarations_listed_as_a_toy_file() {
    let dir = tempfile::tempdir().unwrap();
    ppu_cli::unpack(SAMPLE_PPU_JSON, dir.path()).unwrap();
    let manifest = dir.path().join("ppu.json");
    let text = std::fs::read_to_string(&manifest).unwrap().replacen(
        "\"main.lua\"",
        "\"main.lua\", \"ppu.d.luau\"",
        1,
    );
    std::fs::write(&manifest, text).unwrap();
    std::fs::write(dir.path().join("ppu.d.luau"), ppu_core::LUAU_DECLARATIONS).unwrap();
    let error = ppu_cli::pack(dir.path()).unwrap_err().to_string();
    assert!(error.contains("ppu.d.luau"), "{error}");
}

#[test]
fn unpack_rejects_unknown_version() {
    let text = SAMPLE_PPU_JSON.replacen("\"ppu.toys/1\"", "\"ppu.toys/0\"", 1);
    let dir = tempfile::tempdir().unwrap();
    let err = ppu_cli::unpack(&text, dir.path()).unwrap_err();
    assert!(
        err.to_string().contains("version"),
        "unexpected error: {err}"
    );
}

#[test]
fn unpack_rejects_unsafe_file_names_before_writing_anything() {
    let mut value: serde_json::Value = serde_json::from_str(SAMPLE_PPU_JSON).unwrap();
    value["files"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({ "name": "../evil.lua", "source": "-- evil" }));
    let text = serde_json::to_string(&value).unwrap();

    let dir = tempfile::tempdir().unwrap();
    let err = ppu_cli::unpack(&text, dir.path()).unwrap_err();
    assert!(
        err.to_string().contains("unsafe"),
        "unexpected error: {err}"
    );

    assert!(!dir.path().join("ppu.json").exists());
    assert!(!dir.path().join("evil.lua").exists());
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn unpack_rejects_missing_version() {
    let mut value: serde_json::Value = serde_json::from_str(SAMPLE_PPU_JSON).unwrap();
    value.as_object_mut().unwrap().remove("version");
    let text = serde_json::to_string(&value).unwrap();

    let dir = tempfile::tempdir().unwrap();
    let err = ppu_cli::unpack(&text, dir.path()).unwrap_err();
    assert!(
        format!("{err:#}").contains("not a ppu.toys file"),
        "unexpected error: {err:#}"
    );
}

#[test]
fn unpack_rejects_bad_payload_before_writing_anything() {
    let mut value: serde_json::Value = serde_json::from_str(SAMPLE_PPU_JSON).unwrap();
    value["sources"][0]["payload"] = serde_json::json!("!!!");
    let text = serde_json::to_string(&value).unwrap();

    let dir = tempfile::tempdir().unwrap();
    let err = ppu_cli::unpack(&text, dir.path()).unwrap_err();
    let msg = format!("{err:#}");
    assert!(msg.contains("sky"), "unexpected error: {msg}");
    assert!(msg.contains("payload"), "unexpected error: {msg}");

    assert!(!dir.path().join("main.lua").exists());
    assert!(!dir.path().join("ppu.json").exists());
}

fn copy_dir_all(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();
    for entry in std::fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let dst_path = dst.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir_all(&entry.path(), &dst_path);
        } else {
            std::fs::copy(entry.path(), &dst_path).unwrap();
        }
    }
}

#[test]
fn pack_rejects_manifest_without_main_lua() {
    let dir = tempfile::tempdir().unwrap();
    copy_dir_all(Path::new(SAMPLE_DIR), dir.path());

    let mut manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.path().join("ppu.json")).unwrap())
            .unwrap();
    manifest["files"] = serde_json::json!(["pokes.lua"]);
    std::fs::write(
        dir.path().join("ppu.json"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();

    let err = ppu_cli::pack(dir.path()).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("main.lua"), "unexpected error: {msg}");
    assert!(
        msg.contains(&dir.path().join("ppu.json").display().to_string()),
        "unexpected error: {msg}"
    );
}

#[test]
fn pack_refuses_the_file_names_a_site_revision_would_be_refused_for() {
    for name in [".wip.luau", "levels/.draft.luau", "PPU.JSON", "aux.luau"] {
        let dir = tempfile::tempdir().unwrap();
        copy_dir_all(Path::new(SAMPLE_DIR), dir.path());
        let mut manifest: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.path().join("ppu.json")).unwrap())
                .unwrap();
        manifest["files"].as_array_mut().unwrap().push(name.into());
        std::fs::write(dir.path().join("ppu.json"), manifest.to_string()).unwrap();
        if let Some(parent) = Path::new(name).parent() {
            std::fs::create_dir_all(dir.path().join(parent)).unwrap();
        }
        std::fs::write(dir.path().join(name), "-- x\n").unwrap();
        let msg = ppu_cli::pack(dir.path()).unwrap_err().to_string();
        assert!(
            msg.contains("unsafe file name") && msg.contains(name),
            "{msg}"
        );
    }
}

#[test]
fn pack_and_unpack_refuse_names_that_differ_only_in_case() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("main.luau"), "function frame(t, f) end\n").unwrap();
    std::fs::write(dir.path().join("Boss.luau"), "-- a\n").unwrap();
    std::fs::write(dir.path().join("boss.luau"), "-- b\n").unwrap();
    let manifest =
        serde_json::json!({"title": "t", "files": ["main.luau", "Boss.luau", "boss.luau"]});
    std::fs::write(dir.path().join("ppu.json"), manifest.to_string()).unwrap();
    let msg = ppu_cli::pack(dir.path()).unwrap_err().to_string();
    assert!(
        msg.contains("only in case") && msg.contains("boss.luau"),
        "{msg}"
    );

    let file = serde_json::json!({"version": "ppu.toys/1", "title": "t", "description": "",
        "files": [{"name": "main.luau", "source": ""}, {"name": "A.luau", "source": "a"},
                  {"name": "a.luau", "source": "b"}], "sources": []});
    let out = tempfile::tempdir().unwrap();
    let msg = ppu_cli::unpack(&file.to_string(), &out.path().join("x"))
        .unwrap_err()
        .to_string();
    assert!(msg.contains("only in case"), "{msg}");
    assert!(!out.path().join("x").exists(), "nothing is written");
}

#[test]
fn pack_rejects_unsafe_source_payload_path() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("main.lua"), "function frame(t, f) end\n").unwrap();
    std::fs::write(
        dir.path().join("ppu.json"),
        serde_json::json!({
            "title": "t",
            "description": "",
            "files": ["main.lua"],
            "sources": [{
                "name": "s",
                "kind": "bg",
                "options": {},
                "meta": {},
                "payload": "../x.bin",
            }],
        })
        .to_string(),
    )
    .unwrap();

    let err = ppu_cli::pack(dir.path()).unwrap_err();
    assert!(
        err.to_string().contains("unsafe"),
        "unexpected error: {err}"
    );
}

// A sample source travels through unpack -> pack untouched: pack/unpack copy
// `kind`, `meta` and the payload bytes verbatim, so a kind they never heard of
// (BRR audio, not an image) needs no CLI change.
#[test]
fn sample_source_round_trips_through_unpack_and_pack() {
    use base64::Engine;
    let pcm: Vec<i16> = (0..640)
        .map(|i| ((i as f64 * std::f64::consts::TAU / 32.0).sin() * 20000.0).round() as i16)
        .collect();
    let opts = ppu_core::ConvertSampleOptions {
        loop_start: Some(96),
    };
    let (payload, meta) = ppu_core::convert_sample(&pcm, &opts).unwrap();
    let bytes = payload.encode();

    let mut value: serde_json::Value = serde_json::from_str(SAMPLE_PPU_JSON).unwrap();
    value["sources"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "name": "kick",
            "kind": "sample",
            "builtinId": null,
            "options": { "loop_start": 96 },
            "meta": serde_json::to_value(&meta).unwrap(),
            "payload": base64::engine::general_purpose::STANDARD.encode(&bytes),
        }));
    let text = serde_json::to_string(&value).unwrap();

    let dir = tempfile::tempdir().unwrap();
    ppu_cli::unpack(&text, dir.path()).unwrap();
    assert_eq!(
        std::fs::read(dir.path().join(".ppu/sources/1.bin")).unwrap(),
        bytes
    );

    let repacked: serde_json::Value =
        serde_json::from_str(&ppu_cli::pack(dir.path()).unwrap()).unwrap();
    assert_eq!(repacked, value);
}

#[test]
fn target_hz_round_trips_through_unpack_and_pack() {
    let mut value: serde_json::Value = serde_json::from_str(SAMPLE_PPU_JSON).unwrap();
    value["targetHz"] = serde_json::json!(50);
    value["romSize"] = serde_json::json!(512);
    let dir = tempfile::tempdir().unwrap();
    ppu_cli::unpack(&serde_json::to_string(&value).unwrap(), dir.path()).unwrap();
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.path().join("ppu.json")).unwrap())
            .unwrap();
    assert_eq!(manifest["targetHz"], 50);
    assert_eq!(manifest["romSize"], 512);
    let repacked: serde_json::Value =
        serde_json::from_str(&ppu_cli::pack(dir.path()).unwrap()).unwrap();
    assert_eq!(repacked, value);
}

// A ppu.json written on Windows may use backslashes; pack writes the body's `/` names, so the
// same project packs identically on every platform and unpacks to the same files.
#[test]
fn backslash_paths_in_a_windows_manifest_pack_to_slash_names_and_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("levels")).unwrap();
    std::fs::create_dir_all(dir.path().join("data")).unwrap();
    std::fs::write(dir.path().join("main.luau"), "require(\"./levels/w1\")\n").unwrap();
    std::fs::write(dir.path().join("levels/w1.luau"), "return {}\n").unwrap();
    std::fs::write(dir.path().join("data/sky.bin"), [7u8, 8, 9]).unwrap();
    std::fs::write(
        dir.path().join("ppu.json"),
        serde_json::json!({
            "title": "t",
            "files": ["main.luau", "levels\\w1.luau"],
            "sources": [{ "name": "sky", "kind": "bg", "payload": "data\\sky.bin" }],
        })
        .to_string(),
    )
    .unwrap();

    let packed = ppu_cli::pack(dir.path()).unwrap();
    let body: serde_json::Value = serde_json::from_str(&packed).unwrap();
    assert_eq!(body["files"][1]["name"], "levels/w1.luau");
    assert_eq!(body["sources"][0]["payload"], "BwgJ");

    let out = tempfile::tempdir().unwrap();
    ppu_cli::unpack(&packed, out.path()).unwrap();
    assert_eq!(
        std::fs::read_to_string(out.path().join("levels").join("w1.luau")).unwrap(),
        "return {}\n"
    );
    assert_eq!(ppu_cli::pack(out.path()).unwrap(), packed);
}

#[test]
fn crlf_sources_round_trip_byte_for_byte() {
    let crlf = "-- saved on Windows\r\nfunction frame(t, f)\r\nend\r\n";
    let mut value: serde_json::Value = serde_json::from_str(SAMPLE_PPU_JSON).unwrap();
    value["files"][0]["source"] = crlf.into();
    let text = serde_json::to_string(&value).unwrap();
    let dir = tempfile::tempdir().unwrap();
    ppu_cli::unpack(&text, dir.path()).unwrap();
    assert_eq!(
        std::fs::read(dir.path().join("main.lua")).unwrap(),
        crlf.as_bytes()
    );

    let body: serde_json::Value =
        serde_json::from_str(&ppu_cli::pack(dir.path()).unwrap()).unwrap();
    assert_eq!(body["files"][0]["source"], crlf);
}

/// A folder's `.wav` is a sample source: pack reads it, mixes it to mono, resamples it to 32 kHz and
/// converts it the way a payload's PCM is converted.
#[test]
fn a_wav_sample_source_packs_to_the_payload_its_pcm_converts_to() {
    let dir = tempfile::tempdir().unwrap();
    ppu_cli::unpack(SAMPLE_PPU_JSON, dir.path()).unwrap();
    let pcm: Vec<i16> = (0..640)
        .map(|i| ((i as f64 * std::f64::consts::TAU / 32.0).sin() * 20000.0).round() as i16)
        .collect();
    std::fs::create_dir_all(dir.path().join("sfx")).unwrap();
    let put = |name: &str, bytes: &[u8]| std::fs::write(dir.path().join(name), bytes).unwrap();
    put(
        "sfx/zap.wav",
        &ppu_core::wav::write(&pcm, 1, 32_000).unwrap(),
    );
    let stereo: Vec<i16> = pcm.iter().flat_map(|&s| [s, s / 2]).collect();
    put(
        "sfx/cd.wav",
        &ppu_core::wav::write(&stereo, 2, 44_100).unwrap(),
    );
    put(
        "sfx/lofi.wav",
        &ppu_core::wav::write(&pcm, 1, 22_050).unwrap(),
    );
    let manifest = dir.path().join("ppu.json");
    let original = std::fs::read_to_string(&manifest).unwrap();
    let with = |sources: serde_json::Value| {
        let mut m: serde_json::Value = serde_json::from_str(&original).unwrap();
        m["sources"]
            .as_array_mut()
            .unwrap()
            .extend(sources.as_array().unwrap().clone());
        std::fs::write(&manifest, m.to_string()).unwrap();
        ppu_cli::pack(dir.path())
    };

    let packed = with(serde_json::json!([
        {"name": "zap", "kind": "sample", "file": "sfx/zap.wav", "options": {"loopStart": 96}},
        {"name": "cd", "kind": "sample", "file": "sfx/cd.wav"},
        {"name": "lofi", "kind": "sample", "file": "sfx/lofi.wav"},
    ]));
    let body: serde_json::Value = serde_json::from_str(&packed.unwrap()).unwrap();
    let source = |name: &str| {
        body["sources"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["name"] == name)
            .unwrap()
            .clone()
    };
    let opts = ppu_core::ConvertSampleOptions {
        loop_start: Some(96),
    };
    let (payload, meta) = ppu_core::convert_sample(&pcm, &opts).unwrap();
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD;
    assert_eq!(source("zap")["payload"], b64.encode(payload.encode()));
    assert_eq!(source("zap")["meta"], serde_json::to_value(&meta).unwrap());
    // 640 frames at 44.1 kHz and 22.05 kHz come out as their length at 32 kHz.
    assert_eq!(source("cd")["meta"]["width"], 464);
    assert_eq!(source("lofi")["meta"]["width"], 929);

    put("sfx/fake.wav", b"\x89PNG\r\n\x1a\n not audio");
    let packed = with(serde_json::json!([
        {"name": "fake", "kind": "sample", "file": "sfx/fake.wav"}
    ]));
    let e = format!("{:#}", packed.unwrap_err());
    assert!(e.contains("sfx/fake.wav") && e.contains("not a WAV"), "{e}");

    // Sound RAM holds 64 KB of BRR: about 3.6 s at 32 kHz.
    put(
        "sfx/long.wav",
        &ppu_core::wav::write(&vec![0; 32_000 * 5], 1, 32_000).unwrap(),
    );
    let packed = with(serde_json::json!([
        {"name": "long", "kind": "sample", "file": "sfx/long.wav"}
    ]));
    let e = format!("{:#}", packed.unwrap_err());
    assert!(
        e.contains("sfx/long.wav") && e.contains("too long for sound RAM"),
        "{e}"
    );
}

/// A folder's `.song.json` is a song source: pack encodes the parsed song; text that is not a song,
/// or a song that would not play, is refused naming the file and the field.
/// A file source's meta is rebuilt from the file on every load, but `icon` (the catalogue icon the
/// Studio's Staff view draws for an uploaded sample) is the author's: pack keeps it from ppu.json,
/// and keeps no stale derived field the file no longer gives.
#[test]
fn a_wav_sample_keeps_the_icon_written_in_ppu_json() {
    let dir = tempfile::tempdir().unwrap();
    ppu_cli::unpack(SAMPLE_PPU_JSON, dir.path()).unwrap();
    let pcm: Vec<i16> = (0..640).map(|i| ((i % 32) * 1000) as i16).collect();
    std::fs::write(
        dir.path().join("zap.wav"),
        ppu_core::wav::write(&pcm, 1, 32_000).unwrap(),
    )
    .unwrap();
    let manifest = dir.path().join("ppu.json");
    let mut m: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&manifest).unwrap()).unwrap();
    m["sources"].as_array_mut().unwrap().push(serde_json::json!(
        {"name": "zap", "kind": "sample", "file": "zap.wav",
         "meta": {"icon": "robot", "width": 9, "cells": []}}
    ));
    std::fs::write(&manifest, m.to_string()).unwrap();

    let body: serde_json::Value =
        serde_json::from_str(&ppu_cli::pack(dir.path()).unwrap()).unwrap();
    let meta = &body["sources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "zap")
        .unwrap()["meta"];
    assert_eq!(meta["icon"], "robot");
    assert_eq!(
        meta["width"], 640,
        "the rebuilt width wins over the written one"
    );
    assert!(
        meta.get("cells").is_none(),
        "a derived field the file does not give is not kept"
    );
}

#[test]
fn a_song_json_source_packs_to_the_encoded_song() {
    let dir = tempfile::tempdir().unwrap();
    ppu_cli::unpack(SAMPLE_PPU_JSON, dir.path()).unwrap();
    let song = serde_json::json!({
        "tempo": 120,
        "rows": [{"sound": "kick"}],
        "patterns": [{"name": "A", "length": 48, "notes": [{"at": 0, "row": 0, "len": 12}]}],
    });
    std::fs::create_dir_all(dir.path().join("music")).unwrap();
    let manifest = dir.path().join("ppu.json");
    let original = std::fs::read_to_string(&manifest).unwrap();
    let mut m: serde_json::Value = serde_json::from_str(&original).unwrap();
    m["sources"].as_array_mut().unwrap().push(
        serde_json::json!({"name": "theme", "kind": "song", "file": "music/theme.song.json"}),
    );
    std::fs::write(&manifest, m.to_string()).unwrap();
    let pack_with = |text: String| {
        std::fs::write(dir.path().join("music/theme.song.json"), text).unwrap();
        ppu_cli::pack(dir.path())
    };

    let body: serde_json::Value =
        serde_json::from_str(&pack_with(song.to_string()).unwrap()).unwrap();
    let theme = body["sources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "theme")
        .unwrap()
        .clone();
    let parsed = ppu_core::song::from_json(&song.to_string()).unwrap();
    let psng = ppu_core::song::encode(&parsed);
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD;
    assert_eq!(
        theme["payload"],
        b64.encode(ppu_core::SourcePayload::Song(psng.clone()).encode())
    );
    assert_eq!(theme["meta"]["report"]["report"]["bytes"], psng.len());

    let mut broken = song.clone();
    broken.as_object_mut().unwrap().remove("tempo");
    let err = format!("{:#}", pack_with(broken.to_string()).unwrap_err());
    assert!(
        err.contains("music/theme.song.json") && err.contains("tempo"),
        "{err}"
    );
    let mut broken = song.clone();
    broken["rows"][0]["vol"] = 200.into();
    let err = format!("{:#}", pack_with(broken.to_string()).unwrap_err());
    assert!(
        err.contains("music/theme.song.json") && err.contains("row 1: vol"),
        "{err}"
    );
}

/// A folder's `.mid` is a song source: pack encodes the song core converts it to; bytes that are
/// not a MIDI file are refused naming the file.
#[test]
fn a_mid_source_packs_to_the_converted_song() {
    let dir = tempfile::tempdir().unwrap();
    ppu_cli::unpack(SAMPLE_PPU_JSON, dir.path()).unwrap();
    std::fs::create_dir_all(dir.path().join("music")).unwrap();
    let manifest = dir.path().join("ppu.json");
    let mut m: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&manifest).unwrap()).unwrap();
    m["sources"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"name": "theme", "kind": "song", "file": "music/theme.mid"}));
    std::fs::write(&manifest, m.to_string()).unwrap();
    // ppq 480, one quarter-note C4.
    let mut mid = b"MThd\0\0\0\x06\0\0\0\x01\x01\xe0MTrk\0\0\0\x0d".to_vec();
    mid.extend([0, 0x90, 60, 100, 0x83, 0x60, 0x80, 60, 0, 0, 0xff, 0x2f, 0]);
    let pack_with = |bytes: &[u8]| {
        std::fs::write(dir.path().join("music/theme.mid"), bytes).unwrap();
        ppu_cli::pack(dir.path())
    };

    let body: serde_json::Value = serde_json::from_str(&pack_with(&mid).unwrap()).unwrap();
    let theme = body["sources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "theme")
        .unwrap()
        .clone();
    let song = ppu_core::import::midi::song_from_midi(&mid).unwrap();
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD;
    assert_eq!(
        theme["payload"],
        b64.encode(ppu_core::SourcePayload::Song(ppu_core::song::encode(&song)).encode())
    );

    let err = format!("{:#}", pack_with(b"not midi at all").unwrap_err());
    assert!(
        err.contains("music/theme.mid") && err.contains("not a MIDI file"),
        "{err}"
    );
}
