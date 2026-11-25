//! Folder helpers `ppu connect` and `ppu art` both use: keeping a backup copy of a folder file,
//! and the palette an art payload's own rows make.

use anyhow::{Context, Result};
use ppu_core::{PaletteSource, SourcePayload};
use std::path::Path;

/// Copy `name` (a path in the folder) into the backup directory; false when there is no such file.
pub(crate) fn back_up(dir: &Path, backup: &str, name: &str) -> Result<bool> {
    let from = dir.join(name);
    if !from.is_file() {
        return Ok(false);
    }
    let to = dir.join(backup).join(name);
    std::fs::create_dir_all(to.parent().expect("a backup path has a directory"))?;
    std::fs::copy(&from, &to)
        .with_context(|| format!("cannot keep a copy of {name} in {backup}"))?;
    Ok(true)
}

/// The palette art's own payload holds: its sub-palettes as rows, the way a `.gpl` writes them.
/// It relies on those rows covering every pixel, as an import's always do; render pads the rows
/// of a payload whose pixels reach past them, and that PNG would not import back equal, so such
/// art lands Raw either way.
pub(crate) fn own_palette(payload: &SourcePayload) -> Result<PaletteSource, String> {
    let (Some(row_size), Some(subs)) = (
        ppu_core::art_row_size(payload),
        ppu_core::art_sub_palettes(payload),
    ) else {
        return Err("its payload is not art".into());
    };
    let mut subs = subs.to_vec();
    if let Some(i) = subs.iter().position(|s| s.len() >= row_size as usize) {
        return Err(format!(
            "palette row {i} holds more colours than a row of {row_size} fits"
        ));
    }
    subs.resize(subs.len().max(1), vec![]);
    Ok(PaletteSource::from_sub_palettes(row_size, &subs))
}
