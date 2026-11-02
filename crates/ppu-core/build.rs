//! The engine's Luau libraries: every lib/<name>.luau, embedded as the module `@ppu/<name>`.
use std::{env, fs, path::Path};

fn main() {
    println!("cargo:rerun-if-changed=lib");
    let mut names: Vec<String> = fs::read_dir("lib")
        .expect("ppu-core/lib exists")
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter_map(|n| n.strip_suffix(".luau").map(str::to_owned))
        .collect();
    names.sort();
    let lib = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap()).join("lib");
    let entries: String = names
        .iter()
        .map(|n| format!("    (\"@ppu/{n}.luau\", include_str!({:?})),\n", lib.join(format!("{n}.luau"))))
        .collect();
    let out = Path::new(&env::var("OUT_DIR").unwrap()).join("libraries.rs");
    fs::write(out, format!("&[\n{entries}]")).unwrap();
}
