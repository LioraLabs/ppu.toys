//! DISABLED by m4/memory (M4 substrate rewrite): this golden exercised the deleted
//! v1 direct-RGBA `Source` model. TODO(m4/mode7): rewrite the fixture against the
//! byte-interleaved Mode 7 VRAM (low byte = tilemap, high byte = 8bpp char),
//! regenerate tests/fixtures/golden_mode7.png, and re-enable.

#[test]
#[ignore = "v1 direct-RGBA golden; rewritten against interleaved VRAM in m4/mode7"]
fn golden_mode7_disabled_pending_interleaved_vram() {}
