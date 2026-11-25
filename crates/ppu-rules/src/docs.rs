//! Where a finding sends its reader: every diagnostic code is a docs note id, so a finding's last
//! line names the docs call that opens its note. The MCP reader's pointer lives in the Studio
//! (web/src/studio/agent/tools.ts, findingText), the one place MCP findings are formatted.

/// A finding's last line for the `ppu` command line, indented under it: `  docs: ppu docs <code>`.
pub fn pointer(code: &str) -> String {
    format!("  docs: ppu docs {code}")
}

#[test]
fn pointers_name_the_code() {
    assert_eq!(pointer("snes.budget"), "  docs: ppu docs snes.budget");
}
