//! The luau-lsp setup beside a Luau toy: the engine's declarations, a typed stub per `@ppu/<name>`
//! library, `.luaurc` and VS Code settings. Works with any editor that runs the stock luau-lsp.
use anyhow::{ensure, Context, Result};
use std::fs;
use std::path::Path;

/// `require("@ppu/name")` resolves through this alias to a stub typed by ppu.d.luau.
const LIBRARY_DIR: &str = ".ppu/lib";

const LUAURC: &str = r#"{
  "aliases": {"ppu": ".ppu/lib"},
  "lint": {"BuiltinGlobalWrite": false, "FunctionUnused": false}
}
"#;

// The old solver reports a write to a declared global (`mode = 7`) as unknown; the new one does not.
const VSCODE_SETTINGS: &str = r#"{
  "luau-lsp.types.definitionFiles": {"@ppu": "ppu.d.luau"},
  "luau-lsp.fflags.enableNewSolver": true,
  "luau-lsp.platform.type": "standard",
  "luau-lsp.sourcemap.enabled": false
}
"#;

const VSCODE_EXTENSIONS: &str = r#"{
  "recommendations": ["johnnymorganz.luau-lsp"]
}
"#;

/// Write the editor files into a project directory. The engine's own files (ppu.d.luau and the
/// library stubs) are always rewritten; `.luaurc` and `.vscode/` are written only when absent,
/// so a refresh never overwrites the author's settings. Returns the paths written.
pub fn write_editor_files(dir: &Path) -> Result<Vec<String>> {
    ensure!(
        dir.join(crate::MANIFEST).is_file(),
        "{} is not a ppu project (no {})",
        dir.display(),
        crate::MANIFEST
    );
    let mut written = Vec::new();
    let mut write = |name: &str, text: &str, owned: bool| -> Result<()> {
        let path = dir.join(name);
        if !owned && path.exists() {
            return Ok(());
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("cannot create {}", parent.display()))?;
        }
        fs::write(&path, text).with_context(|| format!("cannot write {}", path.display()))?;
        written.push(name.to_string());
        Ok(())
    };
    write(
        crate::LUAU_DECLARATIONS_FILE,
        ppu_core::LUAU_DECLARATIONS,
        true,
    )?;
    for (file, _) in ppu_core::frontend::LIBRARIES {
        // "@ppu/hud.luau" -> stub ".ppu/lib/hud.luau" typed `ppu_hud`.
        let name = file.trim_start_matches("@ppu/").trim_end_matches(".luau");
        let stub = format!(
            "-- @ppu/{name} for editors: the engine ships the code, ppu.d.luau declares ppu_{name}.\n-- Rewritten by ppu types.\nreturn (nil :: any) :: ppu_{name}\n"
        );
        write(&format!("{LIBRARY_DIR}/{name}.luau"), &stub, true)?;
    }
    write(".luaurc", LUAURC, false)?;
    write(".vscode/settings.json", VSCODE_SETTINGS, false)?;
    write(".vscode/extensions.json", VSCODE_EXTENSIONS, false)?;
    Ok(written)
}
