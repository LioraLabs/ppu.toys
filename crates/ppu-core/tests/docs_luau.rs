//! The API chapters' examples are Luau: every ```luau fence parses with the Luau grammar
//! (the engine would otherwise quietly fall back to its Lua parser) and runs in the preview.
//! docs/audio.md's fences also run in tutorial_first_note.rs, with its songs registered.
mod common;

use ppu_core::{convert_source, ConvertOptions, LuaEngine, SourceKind};

const CHAPTERS: &[(&str, &str)] = &[
    ("luau", include_str!("../../ppu-cli/docs/luau.md")),
    ("cli", include_str!("../../ppu-cli/docs/cli.md")),
    ("registers", include_str!("../../ppu-cli/docs/registers.md")),
    ("display", include_str!("../../ppu-cli/docs/display.md")),
    ("backgrounds", include_str!("../../ppu-cli/docs/backgrounds.md")),
    ("sprites", include_str!("../../ppu-cli/docs/sprites.md")),
    ("mode7", include_str!("../../ppu-cli/docs/mode7.md")),
    ("scanlines", include_str!("../../ppu-cli/docs/scanlines.md")),
    ("windows", include_str!("../../ppu-cli/docs/windows.md")),
    ("color-math", include_str!("../../ppu-cli/docs/color-math.md")),
    ("sources", include_str!("../../ppu-cli/docs/sources.md")),
    ("pixel-art", include_str!("../../ppu-cli/docs/pixel-art.md")),
    ("dma", include_str!("../../ppu-cli/docs/dma.md")),
    ("pad", include_str!("../../ppu-cli/docs/pad.md")),
    ("sram", include_str!("../../ppu-cli/docs/sram.md")),
    ("scenes", include_str!("../../ppu-cli/docs/scenes.md")),
    ("audio", include_str!("../../ppu-cli/docs/audio.md")),
];

/// `(tag, line, body)` for every fenced block.
fn fences(chapter: &str) -> Vec<(&str, usize, &str)> {
    let mut out = vec![];
    let mut lines = chapter.lines().enumerate();
    while let Some((n, line)) = lines.next() {
        let Some(tag) = line.strip_prefix("```") else { continue };
        let start = chapter.lines().take(n + 1).map(|l| l.len() + 1).sum::<usize>();
        let len: usize = lines
            .by_ref()
            .take_while(|(_, l)| !l.starts_with("```"))
            .map(|(_, l)| l.len() + 1)
            .sum();
        out.push((tag.trim(), n + 1, &chapter[start..start + len]));
    }
    out
}

#[test]
fn examples_are_luau_and_lua_appears_only_on_the_luau_page() {
    let mut count = 0;
    for (name, chapter) in CHAPTERS {
        for (tag, line, body) in fences(chapter) {
            assert!(
                tag != "lua" || *name == "luau",
                "docs/{name}.md:{line}: examples are ```luau; Lua belongs on the Luau and Lua page"
            );
            if tag == "luau" {
                count += 1;
                if let Err(e) = full_moon::parse_fallible(body, full_moon::LuaVersion::luau()).into_result() {
                    panic!("docs/{name}.md:{line} is not Luau: {e:?}");
                }
            }
        }
    }
    assert!(count > 60, "only {count} luau fences");
}

/// A distinct 16x16 cell per index, so the OBJ importer keeps every cell.
fn cells(columns: u32, rows: u32) -> (Vec<u8>, u32, u32) {
    sized_cells(columns, rows, 16, 16)
}

/// A distinct cw x ch cell per index.
fn sized_cells(columns: u32, rows: u32, cw: u32, ch: u32) -> (Vec<u8>, u32, u32) {
    let (w, h) = (columns * cw, rows * ch);
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let cell = (y / ch) * columns + x / cw;
            let (cx, cy) = (x % cw, y % ch);
            // A border plus the cell number in binary along the top row.
            let on = cx == 0 || cy == ch - 1 || (cy == 1 && cx >= 2 && cx < 10 && (cell >> (cx - 2)) & 1 == 1);
            let i = ((y * w + x) * 4) as usize;
            if on {
                rgba[i..i + 4].copy_from_slice(&[240, 200, 64, 255]);
            }
        }
    }
    (rgba, w, h)
}

fn picture(w: u32, h: u32) -> Vec<u8> {
    (0..w * h)
        .flat_map(|i| {
            let (x, y) = (i % w, i / w);
            [(x / 8 * 40 % 256) as u8, (y / 8 * 30 % 256) as u8, 96, 255]
        })
        .collect()
}

fn add(e: &mut LuaEngine, name: &str, kind: SourceKind, rgba: &[u8], w: u32, h: u32, opts: ConvertOptions) {
    let (payload, _) = convert_source(kind, &opts, rgba, w, h).unwrap();
    e.add_source(name, &payload.encode()).unwrap();
}

/// The sources the chapters place by name, with stand-in pictures.
fn engine() -> LuaEngine {
    let mut e = LuaEngine::new();
    let bg = |depth| ConvertOptions { bit_depth: Some(depth), ..Default::default() };
    for name in ["sky", "hills"] {
        add(&mut e, name, SourceKind::Bg, &picture(256, 256), 256, 256, bg(4));
    }
    for name in ["tiles", "font"] {
        add(&mut e, name, SourceKind::Sheet, &picture(128, 64), 128, 64, bg(4));
    }
    let (sheet, w, h) = cells(8, 2);
    for name in ["hero", "foes", "ship"] {
        let opts = ConvertOptions { cell_size: Some(16), ..Default::default() };
        add(&mut e, name, SourceKind::Obj, &sheet, w, h, opts);
    }
    // The sprites chapter's sizes: 8x8, 32x32 and 64x32 cells.
    for (name, columns, cw, ch) in [("shots", 4, 8, 8), ("planes", 2, 32, 32), ("barges", 1, 64, 32)] {
        let (sheet, w, h) = sized_cells(columns, 1, cw, ch);
        let opts = ConvertOptions { cell_size: Some(cw as u8), cell_height: Some(ch as u8), ..Default::default() };
        add(&mut e, name, SourceKind::Obj, &sheet, w, h, opts);
    }
    add(&mut e, "floor", SourceKind::M7, &picture(128, 128), 128, 128, ConvertOptions::default());
    common::add_sample(&mut e, "kick");
    e
}

/// Run `program` as main.luau beside `modules`, the chapter's earlier fences that name
/// their file on the first line (`-- state.luau`), so `require("./state")` finds them.
/// Only the modules it requires go along.
fn runs(program: &str, modules: &[(String, &str)]) -> Result<(), String> {
    let mut e = engine();
    let mut files: Vec<(&str, &str)> = modules.iter().map(|(n, s)| (n.as_str(), *s)).collect();
    files.push(("main.luau", program));
    // A module the program does not require would run as a file of its own.
    let required = ppu_core::frontend::required_files(&files);
    files.retain(|(n, _)| *n == "main.luau" || required.contains(*n));
    e.set_sources(&files).map_err(|e| format!("{e:?}"))?;
    for f in 0..3u32 {
        // A is held on frame 1, so pad-gated branches run too.
        e.set_pad(if f == 1 { 1 << 4 } else { 0 });
        e.frame(f as f64 / 60.0, f).map_err(|e| format!("frame {f}: {e:?}"))?;
    }
    Ok(())
}

/// A fence is a whole program, or a fragment of one: setup (run at the top level) or
/// per-frame code (run inside `frame(t, f)`).
#[test]
fn every_luau_example_runs_in_the_preview() {
    for (name, chapter) in CHAPTERS {
        if *name == "audio" {
            continue; // tutorial_first_note.rs runs these with the chapter's songs
        }
        let mut modules: Vec<(String, &str)> = vec![];
        for (tag, line, body) in fences(chapter) {
            if tag != "luau" {
                continue;
            }
            let file = body.lines().next().and_then(|l| l.strip_prefix("-- ")).filter(|l| l.ends_with(".luau"));
            if let Some(file) = file.filter(|f| *f != "main.luau") {
                modules.push((file.to_string(), body));
                continue;
            }
            let whole = runs(body, &modules);
            if whole.is_ok() || body.contains("function frame") {
                whole.unwrap_or_else(|e| panic!("docs/{name}.md:{line}: {e}"));
                continue;
            }
            let framed = format!("function frame(t, f)\n{body}\nend\n");
            if let Err(e) = runs(&framed, &modules) {
                panic!("docs/{name}.md:{line}: {}\nin frame(): {e}", whole.unwrap_err());
            }
        }
    }
}
