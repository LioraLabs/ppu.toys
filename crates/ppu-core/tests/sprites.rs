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
fn sorted_sprites_order_by_layer_then_depth_then_draw_order_and_hide_the_rest() {
    let e = run(
        r#"local dot = sprites.art({{size = 16, columns = 1, cells = {1}}}, sheet)
local big = sprites.art({{size = 8, columns = 1, cells = {3}}, {size = 32, columns = 2, cells = {4, 5, 6, 7}}}, sheet)
function frame(t, f)
  sprites.draw(dot, 10, 10, {layer = 1, depth = 5})
  -- Sorting applies to the whole frame, the draws before the call too.
  sprites.sort()
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
fn unsorted_sprites_take_slots_in_draw_order_whatever_their_layer_and_depth() {
    let e = run(
        r#"local dot = sprites.art({{size = 16, columns = 1, cells = {1}}}, sheet)
function frame(t, f)
  if f == 1 then sprites.sort() end
  sprites.draw(dot, 10, 10, {layer = 1, depth = 5})
  sprites.draw(dot, 20, 10, {layer = 0, depth = 9})
  sprites.draw(dot, 30, 10, {layer = 0, depth = -3})
end"#,
        3,
    );
    // Frame 2 did not sort (frame 1 did): first drawn, lowest slot.
    let xs: Vec<i16> = e.memory().oam[..3].iter().map(|o| o.x).collect();
    assert_eq!(xs, [2, 12, 22]);
}

#[test]
fn raw_slots_stay_the_programs_and_overflow_drops_the_back() {
    let e = run(
        r#"local dot = sprites.art({{size = 16, columns = 1, cells = {1}}}, sheet)
function frame(t, f)
  obj[0].x, obj[0].on = 77, true
  if f == 1 then sprites.sort() end
  for i = 1, 140 do sprites.draw(dot, i, 50, {depth = -i}) end
end"#,
        2,
    );
    let oam = &e.memory().oam;
    assert_eq!((oam[0].x, oam[0].on), (77, true));
    // Sorted: the first 128 draws, the nearest (most negative depth) first; 127 managed slots drop draw 1.
    assert_eq!((oam[1].x, oam[127].x), (120, -6));
    // Unsorted: draw order; draws 128 and later are dropped.
    let e = run(r#"local dot = sprites.art({{size = 16, columns = 1, cells = {1}}}, sheet)
function frame(t, f)
  obj[0].x, obj[0].on = 77, true
  for i = 1, 140 do sprites.draw(dot, i, 50, {depth = -i}) end
end"#, 1);
    let oam = &e.memory().oam;
    assert_eq!((oam[1].x, oam[127].x), (-7, 119));
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

const KEYED: &str = r#"local dot = sprites.art({{size = 16, columns = 1, cells = {1}}}, sheet)
local big = sprites.art({{size = 8, columns = 1, cells = {3}}, {size = 32, columns = 2, cells = {4, 5, 6, 7}}}, sheet)
"#;

#[test]
fn a_key_owns_a_block_from_the_last_slots_and_keep_leaves_it_as_it_was() {
    let program = format!(
        r#"{KEYED}function frame(t, f)
  obj[127].on = true
  sprites.draw(dot, 10 + f, 10)
  -- Key 3 is first drawn in frame 0: four slots (big's largest level), 123..126.
  if f == 0 or f == 2 then sprites.draw(big, 100 + f, 100, {{key = 3, size = 40 - f * 20}}) elseif f < 4 then sprites.keep(3) end
  -- Key 9 first drawn in frame 1 takes the slot above: 122.
  if f >= 1 then sprites.draw(dot, 50, 60 + f, {{key = 9}}) end
end"#
    );
    let mut e = LuaEngine::new();
    e.set_source(&format!("{SHEET}{program}")).unwrap();
    let mut oams = vec![];
    for f in 0..5 {
        e.frame(f as f64 / 60., f).unwrap();
        oams.push(e.memory().oam.clone());
    }
    // Frame 0: the unkeyed dot still takes slot 0; the block shows big's 2x2 level in draw order of cells.
    let o = &oams[0];
    assert_eq!((o[0].x, o[0].on), (2, true));
    assert_eq!(o[123..127].iter().map(|s| (s.x, s.y, s.tile)).collect::<Vec<_>>(), [(84, 84, 8), (100, 84, 10), (84, 100, 12), (100, 100, 14)]);
    // Frame 1: kept, unchanged; key 9's block is the slot above key 3's.
    assert_eq!(oams[1][123..127], o[123..127]);
    assert_eq!((oams[1][122].x, oams[1][122].y, oams[1][122].on), (42, 53, true));
    // Frame 2: a one-cell level fills slot 0 of the block and hides the rest.
    let o = &oams[2];
    assert_eq!((o[123].x, o[123].tile, o[123].on), (94, 6, true));
    assert!(o[124..127].iter().all(|s| !s.on));
    // Frame 4: key 3 neither drawn nor kept: hidden. The raw slot 127 is never a block's.
    assert!(oams[4][123..127].iter().all(|s| !s.on) && oams[4][122].on && oams[4][127].on);
}

#[test]
fn unkeyed_sprites_leave_the_blocks_alone_from_the_next_frame() {
    let mut e = LuaEngine::new();
    e.set_source(&format!(
        "{SHEET}{KEYED}function frame(t, f)\n  for i = 1, 130 do sprites.draw(dot, i, 50) end\n  sprites.draw(big, 60, 60, {{key = 0}})\nend"
    ))
    .unwrap();
    e.frame(0., 0).unwrap();
    // Frame 0: unkeyed draws fill every slot, then key 0's block (124..127) shows over them.
    let o = &e.memory().oam;
    assert_eq!((o[123].x, o[124].x, o[124].tile), (116, 44, 8));
    e.frame(1. / 60., 1).unwrap();
    // Frame 1: unkeyed sprites take slots 0..123 only.
    let o = &e.memory().oam;
    assert_eq!((o[123].x, o[124].x), (116, 44));
}

#[test]
fn hold_keeps_keyed_sprites_too() {
    let mut e = LuaEngine::new();
    e.set_source(&format!(
        "{SHEET}{KEYED}function frame(t, f)\n  if f == 1 then sprites.keep(0) sprites.hold() return end\n  if f == 2 then sprites.keep(0) return end\n  sprites.draw(dot, 30, 30, {{key = 0}})\nend"
    ))
    .unwrap();
    for f in 0..3 {
        e.frame(f as f64 / 60., f).unwrap();
        assert!(e.memory().oam[127].on, "frame {f}");
    }
}

/// The preview's runtime errors carry no line: compare the frame and message.
fn keyed_error(body: &str, message: &str) {
    let mut e = LuaEngine::new();
    e.set_source(&format!("{SHEET}{KEYED}function frame(t, f)\n{body}\nend")).unwrap();
    for f in 0..3 {
        if let Err(err) = e.frame(f as f64 / 60., f) {
            assert!(err.message.contains(message), "{}", err.message);
            assert_eq!(f, 2, "{}", err.message);
            return;
        }
    }
    panic!("no error: {body}");
}

#[test]
fn keyed_errors_name_the_rule() {
    keyed_error("  sprites.draw(dot, 1, 1, {key = 5})\n  if f == 2 then sprites.draw(dot, 2, 2, {key = 5}) end", "sprites key 5 is drawn twice in one frame");
    keyed_error("  if f < 2 then sprites.draw(dot, 1, 1, {key = 5}) else sprites.keep(5) sprites.draw(dot, 1, 1, {key = 5}) end", "both drawn and kept");
    keyed_error("  sprites.draw(dot, 1, 1, {key = 5})\n  if f == 2 then sprites.keep(5) end", "both drawn and kept");
    keyed_error("  if f == 0 then sprites.draw(dot, 1, 1, {key = 5}) else sprites.keep(5) end\n  if f == 2 then sprites.keep(5) end", "kept twice");
    keyed_error("  if f == 2 then sprites.keep(6) end", "sprites.keep(6): the key was never drawn");
    keyed_error("  sprites.draw(f == 2 and big or dot, 1, 1, {key = 5})", "sprites key 5 has 1 slots, from its first art; this art needs 4");
    keyed_error("  sprites.draw(dot, 1, 1, {key = f * 64})", "sprites.draw key must be an integer 0..127");
    keyed_error("  if f == 2 then for k = 0, 32 do sprites.draw(big, 1, 1, {key = k}) end end", "sprites key 32 needs 4 slots; 0 are left to take");
}

#[test]
fn a_keyed_draw_takes_no_layer_or_depth() {
    let err = LuaEngine::new()
        .set_source(&format!("{SHEET}{KEYED}function frame(t, f)\n  sprites.draw(dot, 1, 1, {{key = 1, depth = 2}})\nend"))
        .unwrap_err();
    assert_eq!((err.message.as_str(), err.line), (ppu_core::sprites::KEYED_ORDER, Some(7)));
}

#[test]
fn the_sprites_chapter_keyed_snippet_runs() {
    const CHAPTER: &str = include_str!("../../ppu-cli/docs/sprites.md");
    let section = &CHAPTER[CHAPTER.find("## Keep sprites that did not move").unwrap()..];
    let start = section.find("```lua\n").unwrap() + "```lua\n".len();
    let block = &section[start..start + section[start..].find("```").unwrap()];
    let block = block.replace(r#"dma("ship", {char = 0x4000, pal = 0})"#, "{tile = 0, pal = 0, cells = {{tile = 0}, {tile = 2}, {tile = 4}, {tile = 6}, {tile = 8}, {tile = 10}}}");
    let e = run(&block, 9);
    let oam = &e.memory().oam;
    // Star i (key i, first drawn in that order) sits in slot 128 - i; the unkeyed star takes slot 0.
    assert_eq!((oam[0].x, oam[127].x, oam[120].x), (120, 20, 216));
    assert!(oam[120..].iter().all(|o| o.on) && !oam[1].on);
}
