use anyhow::{bail, Result};

/// `ppu docs [query] [--links]`: a note (by id, code, error text or words) or a map's note list;
/// no query lists the maps.
pub fn docs(query: Option<&str>, links: bool) -> Result<String> {
    let notes = ppu_docs::graph();
    let Some(query) = query else {
        return Ok(format!("Offline documentation bundled with ppu {}.\nUse ppu docs <note id|diagnostic code|error text|words> for one note, or a map for its notes:\n{}", env!("CARGO_PKG_VERSION"), notes.contents()));
    };
    match notes.answer(query, links, ppu_docs::Reader::Shell) {
        Some(text) => Ok(text),
        None => bail!("no note matches {query:?}; ppu docs lists the maps"),
    }
}
