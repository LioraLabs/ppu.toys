/// The prefix of an engine library's require path.
pub const LIBRARY: &str = "@ppu/";

/// A project file that is data, not code: a Tiled map or tileset (`world.load("level1")` reads
/// `level1.tmj`). No reader prepares, requires or runs it as Luau. The XML forms are here so they
/// are refused with "save as JSON" rather than as a Luau syntax error.
pub fn is_project_file(name: &str) -> bool {
    [".tmj", ".tsj", ".tmx", ".tsx"].iter().any(|ext| name.ends_with(ext))
}
