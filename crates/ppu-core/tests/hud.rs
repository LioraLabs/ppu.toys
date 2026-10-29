use ppu_core::LuaEngine;

#[test]
fn hud_helpers_draw_labels_padded_numbers_overflow_and_clear_rectangles() {
    let mut engine = LuaEngine::new();
    engine
        .set_source(
            r#"
local hud = {map=0x7000, width=32, height=4, palette=7}
hud_clear(hud)
hud_text(hud, 1, 1, "SCORE")
hud_number(hud, 7, 1, 36, 6)
hud_number(hud, 20, 1, 1234, 3)
local zeros = {map=0x7000, width=32, height=4, palette=6, padding="0"}
hud_number(zeros, 1, 2, 0, 4)
hud_number(zeros, 7, 2, 2147483647, 10)
hud_text(hud, 31, 3, "!", 5)
local wraps = {map=0x7000, width=32, height=4, palette=7, padding="0", wrap=true}
hud_number(wraps, 20, 2, 1000036, 6)
function frame()
  hud_clear(hud, 0, 0, 32, 1)
end
"#,
        )
        .unwrap();
    engine.frame(0., 0).unwrap();
    let memory = engine.memory();
    let row =
        |y: usize, x: usize, n: usize| &memory.vram[0x7000 + y * 32 + x..0x7000 + y * 32 + x + n];
    assert_eq!(
        row(1, 1, 5),
        &[
            51 + 7 * 1024,
            35 + 7 * 1024,
            47 + 7 * 1024,
            50 + 7 * 1024,
            37 + 7 * 1024
        ]
    );
    assert_eq!(
        row(1, 7, 6),
        &[
            7 * 1024,
            7 * 1024,
            7 * 1024,
            7 * 1024,
            19 + 7 * 1024,
            22 + 7 * 1024
        ]
    );
    assert_eq!(row(1, 20, 3), &[3 + 7 * 1024; 3]);
    assert_eq!(row(2, 1, 4), &[16 + 6 * 1024; 4]);
    let digits: Vec<_> = "2147483647"
        .bytes()
        .map(|b| (b - b'0') as u16 + 16 + 6 * 1024)
        .collect();
    assert_eq!(row(2, 7, 10), digits);
    assert_eq!(row(3, 31, 1), &[1 + 5 * 1024]);
    assert_eq!(
        row(2, 20, 6),
        &[
            16 + 7 * 1024,
            16 + 7 * 1024,
            16 + 7 * 1024,
            16 + 7 * 1024,
            19 + 7 * 1024,
            22 + 7 * 1024
        ]
    );
    assert_eq!(row(0, 0, 32), &[0; 32]);
}

#[test]
fn hud_helpers_reject_invalid_fields_numbers_fonts_and_placements() {
    for (call, message) in [
        ("hud_text(h,31,0,'XX')", "outside its map"),
        ("hud_text(h,0,4,'X')", "outside its map"),
        ("hud_text(h,0.5,0,'X')", "coordinates must be integers"),
        ("hud_text(h,0,0,'é')", "absent"),
        ("hud_text(h,0,0,'X',8)", "palette"),
        ("hud_number(h,0,0,-1,3)", "integer in"),
        ("hud_number(h,0,0,0.5,3)", "integer in"),
        ("hud_number(h,0,0,2147483648,10)", "integer in"),
        ("hud_number(h,0,0,1,0)", "width must be"),
        ("hud_clear(h,0,3,32,2)", "outside its map"),
        ("h.map=32767;hud_clear(h)", "outside VRAM"),
        ("h.width=33;hud_clear(h)", "width must be"),
    ] {
        let mut engine = LuaEngine::new();
        let error = engine
            .set_source(&format!(
                "local h={{map=0x7000,width=32,height=4,palette=7}}\n{call}"
            ))
            .unwrap_err();
        assert!(error.message.contains(message), "{call}: {error:?}");
    }
}
