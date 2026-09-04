use ppu_core::{render_frame, LuaEngine, WIDTH};

#[test]
fn hook_cgram_write_lands_on_its_lines_only() {
    // HDMA to CGRAM: a palette write inside a hook is that line's alone, and
    // never reaches the frame-wide value or the next frame.
    let mut e = LuaEngine::new();
    e.set_source(
        "function frame(t, f) cgram[1] = 0x1111; \
         hdma(10, 20, function(y) cgram[1] = 0x2222 end) end",
    )
    .unwrap();
    let lt = e.frame(0.0, 0).unwrap();
    assert_eq!(lt.rows[5].cgram, vec![]);
    assert_eq!(lt.rows[15].cgram, vec![(1, 0x2222)]);
    assert_eq!(lt.rows[21].cgram, vec![]);
    assert_eq!(e.memory().cgram[1], 0x1111);
    let lt2 = e.frame(1.0, 1).unwrap();
    assert_eq!(lt2.rows[5].cgram, vec![]);
    assert_eq!(e.memory().cgram[1], 0x1111);
}

#[test]
fn hook_backdrop_write_renders_on_its_lines_only() {
    let mut e = LuaEngine::new();
    e.set_source(
        "function frame(t, f) brightness = 15; cgram[0] = rgb(0, 0, 255); \
         hdma(100, 110, function(y) cgram[0] = rgb(255, 0, 0) end) end",
    )
    .unwrap();
    let lt = e.frame(0.0, 0).unwrap();
    let fb = render_frame(&lt, e.memory());
    let px = |y: usize| &fb[(y * WIDTH) * 4..(y * WIDTH) * 4 + 3];
    assert_eq!(
        px(50)[2] > 200 && px(50)[0] < 20,
        true,
        "blue above the band"
    );
    assert_eq!(
        px(105)[0] > 200 && px(105)[2] < 20,
        true,
        "red inside the band"
    );
    assert_eq!(
        px(150)[2] > 200 && px(150)[0] < 20,
        true,
        "blue below the band"
    );
}
