use std::path::{Path, PathBuf};

/// Embeds every note in ppu/docs/notes as `NOTES` and every map in ppu/docs/maps as `MAPS` (file
/// name, text) and, in a checkout that has them, every template's .luau file as `TEMPLATES` (path
/// from the repository root, text): an installed binary needs no checkout, and a new note needs
/// no list.
fn main() {
    // read when the script runs, not baked in: CI reuses a build script across checkout paths
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let here = Path::new(&dir);
    // A checkout reads ppu/docs directly; the packaged crate has the `docs` symlink's files.
    let repo = here.join("../../docs");
    let docs = if repo.is_dir() {
        repo
    } else {
        here.join("docs")
    };
    let mut out = embed(&docs, "notes", "NOTES");
    out += &embed(&docs, "maps", "MAPS");

    // The monorepo's templates sit beside ppu/; the public engine mirror has none, so no examples.
    let root = here.join("../../..");
    let templates = root.join("templates");
    let mut luau = vec![];
    if templates.is_dir() {
        println!("cargo:rerun-if-changed={}", templates.display());
        let mut dirs = vec![templates];
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    println!("cargo:rerun-if-changed={}", path.display());
                    dirs.push(path);
                } else if path.extension().is_some_and(|e| e == "luau") {
                    luau.push(path);
                }
            }
        }
    }
    luau.sort();
    out += "pub(crate) const TEMPLATES: &[(&str, &str)] = &[\n";
    for path in &luau {
        println!("cargo:rerun-if-changed={}", path.display());
        let rel = path
            .strip_prefix(&root)
            .unwrap()
            .to_str()
            .unwrap()
            .replace('\\', "/");
        out += &format!(
            "    ({rel:?}, include_str!({:?})),\n",
            path.display().to_string()
        );
    }
    out += "];\n";
    std::fs::write(
        Path::new(&std::env::var("OUT_DIR").unwrap()).join("embedded.rs"),
        out,
    )
    .unwrap();
}

/// `pub(crate) const <name>: &[(file name, text)]` of every .md file in `docs/<dir>`.
fn embed(docs: &Path, dir: &str, name: &str) -> String {
    let dir = docs.join(dir);
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|f| f.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "md"))
        .collect();
    files.sort();
    let mut out = format!("pub(crate) const {name}: &[(&str, &str)] = &[\n");
    for path in &files {
        println!("cargo:rerun-if-changed={}", path.display());
        let file = path.file_name().unwrap().to_str().unwrap();
        out += &format!(
            "    ({file:?}, include_str!({:?})),\n",
            path.display().to_string()
        );
    }
    out + "];\n"
}
