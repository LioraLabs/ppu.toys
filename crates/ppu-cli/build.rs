use std::path::Path;

/// Embeds ppu/docs/AGENTS.md with every file under ppu/docs/skills as `AGENT_FILES`: installed
/// binaries need no checkout, and a new skill file needs no list.
fn main() {
    // read when the script runs, not baked in: CI reuses a build script across checkout paths
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let here = Path::new(&dir);
    // A checkout reads ppu/docs directly: a Windows checkout has `docs` as a text file, not a
    // symlink. The packaged crate has no ../../docs; `cargo package` copied the `docs`
    // symlink's files into it.
    let repo = here.join("../../docs");
    let docs = if repo.is_dir() {
        repo
    } else {
        here.join("docs")
    };
    agent_files(&docs);
}

/// `AGENT_FILES`: (path under ppu/docs, text) for AGENTS.md and every skill file, each skill a
/// `skills/<name>/` directory whose SKILL.md frontmatter names it and describes it.
fn agent_files(docs: &Path) {
    let mut files = vec![docs.join("AGENTS.md")];
    let skills = docs.join("skills");
    let mut dirs = vec![skills.clone()];
    while let Some(dir) = dirs.pop() {
        println!("cargo:rerun-if-changed={}", dir.display());
        for entry in std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display())) {
            let path = entry.unwrap().path();
            if path.is_dir() {
                dirs.push(path);
            } else {
                files.push(path);
            }
        }
    }
    files.sort();
    for name in std::fs::read_dir(&skills)
        .unwrap()
        .map(|e| e.unwrap().file_name())
    {
        let name = name.to_str().unwrap();
        let skill = skills.join(name).join("SKILL.md");
        let text = std::fs::read_to_string(&skill)
            .unwrap_or_else(|e| panic!("{}: {e}", skill.display()))
            .replace("\r\n", "\n"); // a Windows checkout has CRLF
        assert!(
            text.starts_with(&format!("---\nname: {name}\ndescription: ")),
            "{}: frontmatter must open with name: {name} and a description",
            skill.display()
        );
    }
    let mut out = String::from("const AGENT_FILES: &[(&str, &str)] = &[\n");
    for path in &files {
        println!("cargo:rerun-if-changed={}", path.display());
        let rel = path
            .strip_prefix(docs)
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
        Path::new(&std::env::var("OUT_DIR").unwrap()).join("agent_files.rs"),
        out,
    )
    .unwrap();
}
