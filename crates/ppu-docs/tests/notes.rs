//! Every note holds: it parses, its inline links name notes that exist, its luau snippet runs
//! in the preview (in its `fixture:` template's ppu.json and assets when it names one), and its sh
//! snippet runs in a copy of a template folder with this checkout's `ppu`. Every failure is
//! collected and reported in one run, so a pile of broken notes is fixed at once.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            copy_dir(&path, &to.join(entry.file_name()))?;
        } else {
            std::fs::copy(&path, to.join(entry.file_name()))?;
        }
    }
    Ok(())
}

/// Runs a snippet as a toy for two seconds: its main.luau, or the files its `-- <name>.luau` header
/// lines name. An empty `frame` is added to main.luau when no file has one.
fn run(snippet: &str, fixture: Option<&Path>) -> Result<(), String> {
    let mut files = ppu_docs::Fence { tag: "luau".into(), body: snippet.into() }.files();
    if !files.iter().any(|(name, _)| name == "main.luau") {
        return Err("a fence of several files needs a -- main.luau".into());
    }
    if !files.iter().any(|(_, text)| text.contains("function frame")) {
        let main = files.iter_mut().find(|(name, _)| name == "main.luau").unwrap();
        main.1 += "\nfunction frame(t, f) end\n";
    }
    let names: Vec<&str> = files.iter().map(|(name, _)| name.as_str()).collect();
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    match fixture {
        // The template's sources and assets, none of its code: the snippet is the whole program.
        Some(template) => {
            copy_dir(template, dir.path()).map_err(|e| e.to_string())?;
            let path = dir.path().join("ppu.json");
            let mut manifest: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&path).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
            // Its maps and tilesets stay listed; its Luau files give way to the snippet's.
            let mut listed: Vec<serde_json::Value> = names.iter().map(|n| (*n).into()).collect();
            listed.extend(manifest["files"].as_array().into_iter().flatten().filter(|f| {
                f.as_str().is_some_and(|f| !f.ends_with(".luau") && !f.ends_with(".lua"))
            }).cloned());
            manifest["files"] = listed.into();
            std::fs::write(&path, manifest.to_string()).map_err(|e| e.to_string())?;
        }
        None => std::fs::write(
            dir.path().join("ppu.json"),
            serde_json::json!({"title": "note", "files": names, "sources": []}).to_string(),
        )
        .map_err(|e| e.to_string())?,
    }
    for (name, text) in &files {
        std::fs::write(dir.path().join(name), text).map_err(|e| e.to_string())?;
    }
    let path = dir.path().to_path_buf();
    std::panic::catch_unwind(|| ppu_cli::check(&path, 2.0, &[], None, false, &[]))
        .map_err(|_| "the preview panicked".to_string())?
        .map_err(|e| format!("{e:#}"))
}

/// `PPU_BIN` (cook passes an optimised build), else builds this checkout's `ppu` and returns its path. The build
/// runs cargo inside cargo, so the tests that call it are ignored in a plain `cargo test`: rebuilt under its doctests,
/// ppu-core's rlib changed under rustdoc (E0460).
fn ppu_binary() -> Result<PathBuf, String> {
    if let Some(given) = std::env::var_os("PPU_BIN") {
        return std::path::absolute(given).map_err(|e| e.to_string());
    }
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("../ppu-cli/Cargo.toml");
    let out = Command::new(env!("CARGO"))
        .args(["build", "--bin", "ppu", "--message-format=json", "--manifest-path"])
        .arg(manifest)
        .stderr(Stdio::inherit())
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err("cargo build --bin ppu failed".into());
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .find_map(|m| m["executable"].as_str().filter(|e| e.contains("ppu")).map(PathBuf::from))
        .ok_or_else(|| "cargo build printed no ppu executable".into())
}

/// Runs an sh snippet with `sh -e` in a copy of `template`, `ppu` first on PATH. The home and the
/// ppu config are empty and the site unroutable, so a snippet never signs in or talks to ppu.toys.
fn run_sh(snippet: &str, template: &Path, ppu: &Path) -> Result<(), String> {
    let home = tempfile::tempdir().map_err(|e| e.to_string())?;
    let dir = home.path().join("toy");
    copy_dir(template, &dir).map_err(|e| e.to_string())?;
    let path = format!(
        "{}:{}",
        ppu.parent().unwrap().display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let mut child = Command::new("sh")
        .args(["-ec", snippet])
        .current_dir(&dir)
        .env("PATH", path)
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .env("PPU_CONFIG_DIR", home.path().join(".ppu"))
        .env("PPU_SITE", "http://127.0.0.1:9")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(300);
    loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            let mut err = String::new();
            std::io::Read::read_to_string(child.stderr.as_mut().unwrap(), &mut err).ok();
            return if status.success() {
                Ok(())
            } else {
                Err(format!("{status}: {}", err.trim()))
            };
        }
        if Instant::now() > deadline {
            child.kill().ok();
            return Err("still running after 300 s".into());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn every_note_parses_and_links_to_notes() {
    let (graph, mut failures) = ppu_docs::load();
    for note in graph.notes.iter().filter(|n| n.kind != ppu_docs::Kind::Api) {
        for link in note.links() {
            if graph.get(&link.target).is_none() {
                failures.push(format!(
                    "notes/{}.md: [[{}]] names no note",
                    note.id, link.target
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} note failure(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// The snippet of the note named by `NOTE`, or of every note when it is unset. A plain `cargo test` skips it:
/// `cook test-notes` runs it once per note, optimised, with an optimised `ppu` in `PPU_BIN` (unoptimised,
/// the CLI snippets alone took ten minutes).
#[test]
#[ignore = "cook test-notes runs it once per note"]
fn every_note_runs_its_snippet() {
    let (graph, _) = ppu_docs::load();
    let only = std::env::var("NOTE").ok();
    if let Some(id) = &only {
        assert!(graph.get(id).is_some(), "NOTE={id} names no note");
    }
    let mut failures = vec![];
    let templates = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../templates");
    let mut ran = 0;
    let mut ppu: Option<Result<PathBuf, String>> = None;
    for note in graph.notes.iter().filter(|n| n.kind != ppu_docs::Kind::Api) {
        if only.as_ref().is_some_and(|id| *id != note.id) {
            continue;
        }
        let Some(snippet) = note.snippet() else {
            continue;
        };
        if snippet.tag == "sh" {
            // CLI snippets run in a template folder: the note's fixture, else the blank game.
            let template = templates.join(note.fixture.as_deref().unwrap_or("title"));
            if !templates.is_dir() {
                eprintln!("notes/{}.md: skipped, no templates/ in this checkout", note.id);
            } else if !template.is_dir() {
                failures.push(format!(
                    "notes/{}.md: fixture {:?} is not a template",
                    note.id, note.fixture
                ));
            } else {
                match ppu.get_or_insert_with(ppu_binary) {
                    Err(e) => failures.push(format!("notes/{}.md: {e}", note.id)),
                    Ok(bin) => {
                        ran += 1;
                        if let Err(e) = run_sh(&snippet.body, &template, bin) {
                            failures.push(format!("notes/{}.md: the sh snippet fails: {e}", note.id));
                        }
                    }
                }
            }
            continue;
        }
        if snippet.tag != "luau" {
            continue;
        }
        let fixture = match &note.fixture {
            None => None,
            // The public engine mirror has no templates: its notes with fixtures cannot run there.
            Some(_) if !templates.is_dir() => {
                eprintln!(
                    "notes/{}.md: skipped, no templates/ in this checkout",
                    note.id
                );
                continue;
            }
            Some(name) if !templates.join(name).is_dir() => {
                failures.push(format!(
                    "notes/{}.md: fixture {name:?} is not a template",
                    note.id
                ));
                continue;
            }
            Some(name) => Some(templates.join(name)),
        };
        ran += 1;
        if let Err(e) = run(&snippet.body, fixture.as_deref()) {
            failures.push(format!(
                "notes/{}.md: the snippet fails in the preview: {e}",
                note.id
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} note failure(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
    // A note without a snippet runs nothing: only the whole pile must run something.
    assert!(ran > 0 || only.is_some(), "no snippet ran");
}

#[test]
fn a_missing_link_target_and_a_broken_snippet_are_failures() {
    let note = ppu_docs::parse_note(
        "x.md",
        "---\nid: x\nkind: rule\ntitle: t\n---\nSee [[nowhere|it]].\n\n```luau\nlocal = 1\n```\n",
    )
    .unwrap();
    let (graph, _) = ppu_docs::load();
    assert!(graph.get(&note.links()[0].target).is_none());
    assert!(run(&note.snippet().unwrap().body, None).is_err());
}

#[test]
#[ignore = "cook test-notes-sh runs it with an optimised ppu"]
fn a_broken_sh_snippet_is_a_failure() {
    let templates = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../templates/title");
    if !templates.is_dir() {
        return;
    }
    let ppu = ppu_binary().unwrap();
    assert!(run_sh("ppu check . --duration 0.1\n", &templates, &ppu).is_ok());
    assert!(run_sh("ppu check . --no-such-flag\n", &templates, &ppu).is_err());
}

/// Every engine error code is some note's code: a message's `error[code]` always leads to a note.
#[test]
fn every_engine_code_has_a_note() {
    let graph = ppu_docs::graph();
    let missing: Vec<&str> =
        ppu_core::codes::ALL.iter().copied().filter(|c| graph.by_code(c).is_none()).collect();
    assert!(missing.is_empty(), "{} engine code(s) with no note: {missing:?}", missing.len());
}
