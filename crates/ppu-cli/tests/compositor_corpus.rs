//! Demo-corpus digest for compositor refactors.
//!
//! Plays every toy under `$PPU_DEMOS` (default `$HOME/dev/ppu-demos/toys`) for
//! 180 frames and folds the framebuffer AND the inspector buffers (main, sub,
//! math mask, sprite-overflow stats) of each frame into a per-toy FNV-1a
//! digest. A compositor refactor must reproduce every digest exactly; the
//! corpus covers EXTBG, Mode 7, direct colour, mosaic, windows and colour math.
//!
//! Run:
//! `cargo test --release --manifest-path <ppu>/Cargo.toml -p ppu-cli --test compositor_corpus -- --ignored --nocapture`

use base64::Engine as _;
use ppu_core::{render_frame_view, LuaEngine, CONTROLS_FILE};
use std::path::PathBuf;
use std::time::Instant;

/// Toys this CLI cannot pack: their `sample` sources are WAV files, but `pack`
/// only imports PNG (`invalid PNG signature`).
const SKIP: &[&str] = &["first-note", "pad-piano"];

const FRAMES: u32 = 180;

fn fnv(h: &mut u64, bytes: &[u8]) {
    for &b in bytes {
        *h = (*h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3);
    }
}

fn str_of<'a>(v: &'a serde_json::Value, key: &str, toy: &str) -> &'a str {
    v[key]
        .as_str()
        .unwrap_or_else(|| panic!("{toy}: missing string field {key:?}"))
}

fn toy_digest(dir: &std::path::Path, toy: &str) -> u64 {
    let text = ppu_cli::pack(dir).unwrap_or_else(|e| panic!("{toy}: pack failed: {e:#}"));
    let body: serde_json::Value =
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("{toy}: bad packed json: {e}"));
    let list = |key: &str| -> Vec<serde_json::Value> {
        body[key]
            .as_array()
            .unwrap_or_else(|| panic!("{toy}: missing array {key:?}"))
            .clone()
    };
    let mut e = LuaEngine::new();
    for s in list("sources") {
        let name = str_of(&s, "name", toy);
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(str_of(&s, "payload", toy))
            .unwrap_or_else(|err| panic!("{toy}: bad base64 for source {name:?}: {err}"));
        e.add_source(name, &bytes)
            .unwrap_or_else(|err| panic!("{toy}: source {name:?}: {err:?}"));
    }
    let files = list("files");
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|f| (str_of(f, "name", toy), str_of(f, "source", toy)))
        .collect();
    // Studio supplies the generated controls file; the packed demos omit it, so
    // give toys that call `apply_pokes()` the same empty one (as `ppu new` does).
    let controls = [(CONTROLS_FILE, "function apply_pokes() end")];
    let files: Vec<_> = if files.iter().any(|(n, _)| *n == CONTROLS_FILE) {
        files
    } else {
        controls.into_iter().chain(files).collect()
    };
    e.set_sources(&files).unwrap_or_else(|err| {
        panic!(
            "{toy}: set_sources: {}: {}",
            err.file.as_deref().unwrap_or("Lua"),
            err.message
        )
    });

    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for f in 0..FRAMES {
        let lt = e.frame(f64::from(f) / 60.0, f).unwrap_or_else(|err| {
            panic!(
                "{toy}: frame {f}: {}:{:?}: {}",
                err.file.as_deref().unwrap_or("Lua"),
                err.line,
                err.message
            )
        });
        let v = render_frame_view(&lt, e.memory());
        for buf in [&v.framebuffer, &v.main, &v.sub, &v.math_mask] {
            fnv(&mut h, buf);
        }
        let o = &v.overflow;
        fnv(&mut h, &[u8::from(o.range_over), u8::from(o.time_over)]);
        fnv(&mut h, &(o.max_sprites as u64).to_le_bytes());
        fnv(&mut h, &(o.max_tiles as u64).to_le_bytes());
    }
    h
}

#[test]
#[ignore]
fn compositor_corpus_digest() {
    let root = std::env::var_os("PPU_DEMOS")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join("dev/ppu-demos/toys")
        });
    if !root.is_dir() {
        eprintln!("skipping: {} not found (set PPU_DEMOS)", root.display());
        return;
    }
    let mut dirs: Vec<_> = std::fs::read_dir(&root)
        .unwrap()
        .map(|d| d.unwrap().path())
        .filter(|p| p.join("ppu.json").is_file())
        .collect();
    dirs.sort();
    let start = Instant::now();
    let (mut corpus, mut n) = (0xcbf2_9ce4_8422_2325u64, 0);
    for dir in dirs {
        let toy = dir.file_name().unwrap().to_string_lossy().into_owned();
        if SKIP.contains(&toy.as_str()) {
            continue;
        }
        let d = toy_digest(&dir, &toy);
        println!("{toy} {d:016x}");
        fnv(&mut corpus, &d.to_le_bytes());
        n += 1;
    }
    println!("toys={n} elapsed={:.2?}", start.elapsed());
    println!("corpus={corpus:016x}");
}
