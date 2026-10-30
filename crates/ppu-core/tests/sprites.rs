use ppu_core::LuaEngine;

/// A sheet as dma() returns one: cell n is tile 2n.
const SHEET: &str = r#"local cells = {}
for i = 1, 64 do cells[i] = {tile = (i - 1) * 2} end
local sheet = {tile = 0, pal = 2, cells = cells}
"#;

fn run(program: &str, frames: u32) -> LuaEngine {
    let mut engine = LuaEngine::new();
    engine.set_source(&format!("{SHEET}{program}")).unwrap();
    for f in 0..frames {
        engine.frame(f as f64 / 60., f).unwrap();
    }
    engine
}

#[test]
fn sprites_order_by_layer_then_depth_then_draw_order_and_hide_the_rest() {
    let e = run(
        r#"local dot = sprites.art({{size = 16, columns = 1, cells = {1}}}, sheet)
local big = sprites.art({{size = 8, columns = 1, cells = {3}}, {size = 32, columns = 2, cells = {4, 5, 6, 7}}}, sheet)
function frame(t, f)
  sprites.draw(dot, 10, 10, {layer = 1, depth = 5})
  sprites.draw(dot, 20, 10, {layer = 1, depth = 2})
  sprites.draw(dot, 30, 10, {layer = 0, depth = 9})
  sprites.draw(dot, 40, 10, {layer = 1, depth = 2})
  sprites.draw(big, 100, 100, {size = 20, prio = 1})
  sprites.draw(big, 100, 150, {size = 5})
  sprites.draw(dot, 400, 10)
end"#,
        2,
    );
    let oam = &e.memory().oam;
    let xs: Vec<i16> = oam[..9].iter().map(|o| o.x as i16).collect();
    // Layer 0 by depth: big's 32 level (2x2), big's 8 level, the dot at depth 9; then layer 1 by
    // depth and draw order. The dot at 400 is off screen.
    assert_eq!(xs, [84, 100, 84, 100, 92, 22, 12, 32, 2]);
    assert_eq!((oam[0].tile, oam[0].pal, oam[0].prio, oam[0].large), (8, 2, 1, true));
    assert!(oam[..9].iter().all(|o| o.on));
    assert!(oam[9..].iter().all(|o| !o.on));
}

#[test]
fn raw_slots_stay_the_programs_and_overflow_drops_the_back() {
    let e = run(
        r#"local dot = sprites.art({{size = 16, columns = 1, cells = {1}}}, sheet)
function frame(t, f)
  obj[0].x, obj[0].on = 77, true
  for i = 1, 140 do sprites.draw(dot, i, 50, {depth = -i}) end
end"#,
        1,
    );
    let oam = &e.memory().oam;
    assert_eq!((oam[0].x, oam[0].on), (77, true));
    // 127 managed slots: the nearest (most negative depth) first; draws 13 and below are dropped.
    assert_eq!(oam[1].x, 132);
    assert_eq!(oam[127].x, 6);
}

#[test]
fn the_sprites_chapter_snippet_runs() {
    const CHAPTER: &str = include_str!("../../ppu-cli/docs/sprites.md");
    let section = &CHAPTER[CHAPTER.find("## Draw sprites in order").unwrap()..];
    let start = section.find("```lua\n").unwrap() + "```lua\n".len();
    let block = &section[start..start + section[start..].find("```").unwrap()];
    // The chapter loads a sheet from a source; stand in a sheet of the same shape.
    let block = block.replace(r#"dma("ship", {char = 0x4000, pal = 0})"#, "{tile = 0, pal = 0, cells = {{tile = 0}, {tile = 2}, {tile = 4}, {tile = 6}, {tile = 8}, {tile = 10}}}");
    let e = run(&block, 2);
    let oam = &e.memory().oam;
    assert_eq!((oam[0].x, oam[1].x), (8, 120));
    assert!(oam[1..12].iter().all(|o| o.on) && !oam[12].on);
}

#[test]
fn hold_keeps_last_frames_sprites_and_refuses_a_draw() {
    let program = r#"local dot = sprites.art({{size = 16, columns = 1, cells = {1}}}, sheet)
function frame(t, f)
  if f == 1 then sprites.hold() return end
  sprites.draw(dot, 40 + f, 50)
  if f == 2 then sprites.hold() end
end"#;
    let mut e = LuaEngine::new();
    e.set_source(&format!("{SHEET}{program}")).unwrap();
    e.frame(0., 0).unwrap();
    let before = e.memory().oam.clone();
    e.frame(1. / 60., 1).unwrap();
    assert_eq!(e.memory().oam, before);
    assert!(before[0].on);
    let err = e.frame(2. / 60., 2).unwrap_err();
    assert!(err.message.contains("sprites.draw and sprites.hold cannot both run in one frame"), "{}", err.message);
}
