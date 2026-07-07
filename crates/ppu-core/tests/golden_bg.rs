//! DISABLED by m4/memory (M4 substrate rewrite): this golden exercised the deleted
//! v1 direct-RGBA `Source` model. TODO(m4/bg-raster): rewrite the fixture against
//! byte-accurate VRAM (tilemap at map_base, bitplane char at char_base, CGRAM
//! sub-palettes), regenerate tests/fixtures/golden_bg.png, and re-enable.

#[test]
#[ignore = "v1 direct-RGBA golden; rewritten against byte-accurate VRAM in m4/bg-raster"]
fn golden_bg_disabled_pending_vram_rasterizer() {}
