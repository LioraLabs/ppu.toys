//! DISABLED by m4/memory (M4 substrate rewrite): this golden exercised the deleted
//! v1 direct-RGBA OBJ sheet. TODO(m4/compositing, m4/demos): sprite pixel sampling is
//! stubbed (see src/sprite.rs); rebuild this fixture when sprites sample real
//! memory again, regenerate tests/fixtures/golden_sprite.png, and re-enable.

#[test]
#[ignore = "v1 direct-RGBA golden; sprite sampling returns with m4/compositing/m4/demos"]
fn golden_sprite_disabled_pending_sprite_sampling() {}
