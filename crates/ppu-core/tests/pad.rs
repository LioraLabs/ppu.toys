use ppu_core::LuaEngine;

#[test]
fn pad_global_mirrors_the_mask_each_frame() {
    let mut e = LuaEngine::new();
    e.set_source(
        "function frame(t, f)\n  brightness = pad.a and 3 or 9\n  if pad.left then mode = 7 end\nend\n",
    )
    .unwrap();
    // all released by default
    assert_eq!(e.frame(0.0, 0).unwrap().rows[0].brightness, 9);
    // bit 4 = a, bit 2 = left (PAD_NAMES order)
    e.set_pad((1 << 4) | (1 << 2));
    let lt = e.frame(0.0, 1).unwrap();
    assert_eq!(lt.rows[0].brightness, 3);
    assert_eq!(lt.rows[0].mode, 7);
    // sticky until cleared
    assert_eq!(e.frame(0.0, 2).unwrap().rows[0].brightness, 3);
    e.set_pad(0);
    assert_eq!(e.frame(0.0, 3).unwrap().rows[0].brightness, 9);
}
