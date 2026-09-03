use std::path::Path;

const SAMPLE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sample");
const SAMPLE_PPU_JSON: &str = include_str!("fixtures/sample.ppu.json");

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
