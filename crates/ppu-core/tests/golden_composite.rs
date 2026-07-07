//! DISABLED by m4/memory (M4 substrate rewrite): this golden exercised the deleted
//! v1 direct-RGBA `Source` model. TODO(m4/compositing): rebuild the split-screen
//! (Mode 1 band over Mode 7 band + sprite) fixture on the VRAM substrate once
//! m4/bg-raster + m4/mode7 land, regenerate tests/fixtures/golden_composite.png, and
//! re-enable together with the real priority/compositing rules.

#[test]
#[ignore = "v1 direct-RGBA golden; rebuilt on the VRAM substrate in m4/compositing"]
fn golden_composite_disabled_pending_compositing() {}
