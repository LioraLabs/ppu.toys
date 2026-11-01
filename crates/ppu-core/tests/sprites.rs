use ppu_core::LuaEngine;

/// A sheet as dma() returns one: cell n is tile 2n.
const SHEET: &str = r#"local cells = {}
for i = 1, 64 do cells[i] = {tile = (i - 1) * 2} end
local sheet = {tile = 0, pal = 2, cells = cells}
"#;

fn run(program: &str, frames: u32) -> LuaEngine {
    run_as("source", program, frames)
}

fn run_as(file: &str, program: &str, frames: u32) -> LuaEngine {
    let mut engine = LuaEngine::new();
    engine.set_sources(&[(file, &format!("{SHEET}{program}"))]).unwrap();
    for f in 0..frames {
        engine.frame(f as f64 / 60., f).unwrap();
    }
    engine
}

#[test]
fn groups_draw_in_the_order_they_are_made_and_size_by_their_draws() {
    let e = run(
        r#"local dot = sprites.art({{size = 16, columns = 1, cells = {1}}}, sheet)
local big = sprites.art({{size = 8, columns = 1, cells = {3}}, {size = 32, columns = 2, cells = {4, 5, 6, 7}}}, sheet)
local top = sprites.group()
local rows = sprites.group()
local function row(y)
  for i = 1, 3 do rows:draw(big, i * 40, y) end
end
function frame(t, f)
  -- Plain draws go to the default group, behind the others, whatever the call order.
  sprites.draw(dot, 5, 5)
  row(100)
  row(150)
  top:draw(dot, 50, 50)
end"#,
        2,
    );
    let oam = &e.memory().oam;
    // top: 1 slot (0); rows: 2 calls x 3 draws x 4 cells = 24 slots (1..24); the default group the rest (25..).
    assert_eq!((oam[0].x, oam[1].x, oam[1].tile, oam[24].x, oam[25].x), (42, 24, 8, 120, -3));
    assert!(!oam[26].on);
}

#[test]
fn a_sorted_group_orders_its_entries_by_depth_each_frame() {
    let e = run(
        r#"local dot = sprites.art({{size = 16, columns = 1, cells = {1}}}, sheet)
local world = sprites.group({sort = true})
function frame(t, f)
  world:draw(dot, 10, 10, {depth = 5})
  world:draw(dot, 20, 10, {depth = 2})
  if f == 0 then world:draw(dot, 30, 10, {depth = 9, key = 1}) else world:keep(1) end
  world:draw(dot, 40, 10, {depth = 2})
end"#,
        2,
    );
    // Nearer first, equal depths in call order; the kept entry in its place by depth.
    let xs: Vec<i16> = e.memory().oam[..4].iter().map(|o| o.x).collect();
    assert_eq!(xs, [12, 32, 2, 22]);
}

#[test]
fn raw_slots_stay_the_programs_and_overflow_drops_the_back() {
    let e = run(r#"local dot = sprites.art({{size = 16, columns = 1, cells = {1}}}, sheet)
function frame(t, f)
  obj[0].x, obj[0].on = 77, true
  for i = 1, 140 do sprites.draw(dot, i, 50) end
end"#, 1);
    let oam = &e.memory().oam;
    assert_eq!((oam[0].x, oam[0].on), (77, true));
    // Draw order; the default group's 127 slots drop draws 128 and later.
    assert_eq!((oam[1].x, oam[127].x), (-7, 119));
}

#[test]
fn the_sprites_chapter_snippets_run() {
    const CHAPTER: &str = include_str!("../../ppu-cli/docs/sprites.md");
    let snippet = |heading: &str| {
        let section = &CHAPTER[CHAPTER.find(heading).unwrap()..];
        let start = section.find("```luau\n").unwrap() + "```luau\n".len();
        // The chapter loads a sheet from a source; stand in a sheet of the same shape.
        section[start..start + section[start..].find("```").unwrap()]
            .replace(r#"dma("ship", {char = 0x4000, pal = 0})"#, "{tile = 0, pal = 0, cells = {{tile = 0}, {tile = 2}, {tile = 4}, {tile = 6}, {tile = 8}, {tile = 10}}}")
            .replace(r#"dma("shots", {char = 0x4000, pal = 0})"#, "{tile = 0, pal = 0, cell_w = 8, cell_h = 8, cells = {{tile = 1, pal = 0}, {tile = 2, pal = 0}}}")
            .replace(r#"dma("planes", {char = 0x4800, pal = 1})"#, "{tile = 128, pal = 1, cell_w = 32, cell_h = 32, cells = {{tile = 0, pal = 0}, {tile = 4, pal = 0}}}")
            .replace(r#"dma("barges", {char = 0x5000, pal = 2})"#, "{tile = 256, pal = 2, cell_w = 64, cell_h = 32, cells = {{tile = 0, pal = 0}}}")
    };
    let e = run_as("main.luau", &snippet("## Sizes, flips and palettes"), 2);
    let m = e.memory();
    assert_eq!(m.obsel.size_sel, 1, "8x8 and 32x32 art: pair 1");
    let s = |i: usize| (m.oam[i].x, m.oam[i].y, m.oam[i].tile, m.oam[i].pal, m.oam[i].large);
    assert_eq!((s(0), s(1), s(2)), ((112, 84, 132, 1, true), (28, 144, 256, 2, true), (60, 144, 260, 2, true)));
    assert_eq!((s(3), s(4), s(6)), ((36, 56, 1, 3, false), (56, 56, 1, 4, false), (96, 56, 1, 4, false)));
    let e = run_as("main.luau", &snippet("## Sizes, flips and palettes"), 17);
    assert!(e.memory().oam[0].flip_x && e.memory().oam[0].flip_y, "heading 2 flips both ways");
    let e = run_as("main.luau", &snippet("## Draw sprites in groups"), 2);
    let oam = &e.memory().oam;
    assert_eq!((oam[0].x, oam[1].x, oam[2].x, oam[6].x, oam[10].x, oam[11].x, oam[18].x), (8, 120, 174, 124, 82, 32, 12));
    assert!(oam[..12].iter().all(|o| o.on) && !oam[12].on && !oam[19].on);
    let e = run_as("main.luau", &snippet("## Keep sprites that did not move"), 9);
    let oam = &e.memory().oam;
    assert_eq!((oam[0].x, oam[1].x, oam[8].x), (120, 20, 216));
    assert!(oam[..9].iter().all(|o| o.on) && !oam[9].on);
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
fn a_key_owns_a_block_from_its_groups_first_slots_and_keep_leaves_it_as_it_was() {
    let program = format!(
        r#"{KEYED}local top = sprites.group()
local g = sprites.group()
function frame(t, f)
  obj[127].on = true
  top:draw(dot, 0, 0)
  g:draw(dot, 10 + f, 10)
  -- Key 3 is first drawn in frame 0: four slots of g (big's largest level), 1..4.
  if f == 0 or f == 2 then g:draw(big, 100 + f, 100, {{key = 3, size = 40 - f * 20}}) elseif f < 4 then g:keep(3) end
  -- Key 9 first drawn in frame 1 takes the slot after: 5.
  if f >= 1 then g:draw(dot, 50, 60 + f, {{key = 9}}) end
end"#
    );
    let mut e = LuaEngine::new();
    e.set_source(&format!("{SHEET}{program}")).unwrap();
    let mut oams = vec![];
    for f in 0..5 {
        e.frame(f as f64 / 60., f).unwrap();
        oams.push(e.memory().oam.clone());
    }
    // g has 1 + 4 + 1 = 6 slots (1..6). Frame 0: the block over the unkeyed dot that took slot 1.
    let o = &oams[0];
    assert_eq!(o[1..5].iter().map(|s| (s.x, s.y, s.tile)).collect::<Vec<_>>(), [(84, 84, 8), (100, 84, 10), (84, 100, 12), (100, 100, 14)]);
    // Frame 1: kept, unchanged; key 9's block is slot 5.
    assert_eq!(oams[1][1..5], o[1..5]);
    assert_eq!((oams[1][5].x, oams[1][5].y, oams[1][5].on), (42, 53, true));
    // Frame 2: a one-cell level fills slot 1 and hides the rest of the block; the unkeyed dot takes slot 6.
    let o = &oams[2];
    assert_eq!((o[1].x, o[1].tile, o[1].on), (94, 6, true));
    assert!(o[2..5].iter().all(|s| !s.on));
    assert_eq!((o[6].x, o[6].on), (4, true));
    // Frame 4: key 3 neither drawn nor kept: hidden. The raw slot 127 is never a group's.
    assert!(oams[4][1..5].iter().all(|s| !s.on) && oams[4][5].on && oams[4][127].on);
}

#[test]
fn hold_keeps_every_group() {
    let mut e = LuaEngine::new();
    e.set_source(&format!(
        "{SHEET}{KEYED}local g = sprites.group({{slots = 4}})\nfunction frame(t, f)\n  if f == 1 then g:keep(0) sprites.hold() return end\n  if f == 2 then g:keep(0) return end\n  g:draw(dot, 30, 30, {{key = 0}})\nend"
    ))
    .unwrap();
    for f in 0..3 {
        e.frame(f as f64 / 60., f).unwrap();
        assert!(e.memory().oam[0].on, "frame {f}");
    }
}

fn keyed_error(body: &str, message: &str) {
    let mut e = LuaEngine::new();
    e.set_source(&format!("{SHEET}{KEYED}local g = sprites.group({{slots = 128}})\nlocal h = sprites.group({{slots = 0}})\nfunction frame(t, f)\n{body}\nend")).unwrap();
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
    keyed_error("  g:draw(dot, 1, 1, {key = 5})\n  if f == 2 then g:draw(dot, 2, 2, {key = 5}) end", "sprites key 5 is drawn twice in one frame");
    keyed_error("  if f < 2 then g:draw(dot, 1, 1, {key = 5}) else g:keep(5) g:draw(dot, 1, 1, {key = 5}) end", "both drawn and kept");
    keyed_error("  g:draw(dot, 1, 1, {key = 5})\n  if f == 2 then g:keep(5) end", "both drawn and kept");
    keyed_error("  if f == 0 then g:draw(dot, 1, 1, {key = 5}) else g:keep(5) end\n  if f == 2 then g:keep(5) end", "kept twice");
    keyed_error("  if f == 2 then g:keep(6) end", "keep(6): the key was never drawn in this group");
    // Keys are the group's: key 5 drawn in g is not h's.
    keyed_error("  g:draw(dot, 1, 1, {key = 5})\n  if f == 2 then h:keep(5) end", "keep(5): the key was never drawn in this group");
    keyed_error("  g:draw(f == 2 and big or dot, 1, 1, {key = 5})", "sprites key 5 has 1 slots, from its first art; this art needs 4");
    keyed_error("  g:draw(dot, 1, 1, {key = f * 64})", "draw key must be an integer 0..127");
    keyed_error("  if f == 2 then for k = 0, 32 do g:draw(big, 1, 1, {key = k}) end end", "sprites key 32 needs 4 slots; 0 are left in its group");
}

fn plan_error(program: &str, line: u32, message: &str) {
    let err = LuaEngine::new().set_source(&format!("{SHEET}{KEYED}{program}")).unwrap_err();
    assert_eq!((err.message.as_str(), err.line), (message, Some(line)), "{program}");
}

#[test]
fn group_rules_are_static_errors_on_their_line() {
    use ppu_core::sprites::{GROUP_PLACE, NO_GROUP, NO_LAYER, NO_SORT, UNSIZED};
    // SHEET is lines 1-3, KEYED 4-5: the program starts on line 6.
    plan_error("function frame(t, f)\n  local g = sprites.group()\n  g:draw(dot, 1, 1)\nend", 7, GROUP_PLACE);
    plan_error("local g = sprites.group()\nfunction frame(t, f)\n  while f > 0 do g:draw(dot, 1, 1) f = f - 1 end\nend", 8, UNSIZED);
    // `c and a or b` (Luau's `if c then a else b` as the preview runs it) sizes for either art; a value
    // that is not art does not.
    plan_error("local g = sprites.group()\nlocal other = {}\nfunction frame(t, f)\n  g:draw(f > 1 and dot or other, 1, 1)\nend", 9, UNSIZED);
    plan_error("function frame(t, f, g)\n  g:draw(dot, 1, 1)\nend", 7, NO_GROUP);
    plan_error("function frame(t, f)\n  sprites.sort()\n  sprites.draw(dot, 1, 1)\nend", 7, NO_SORT);
    plan_error("function frame(t, f)\n  sprites.draw(dot, 1, 1, {layer = 2})\nend", 7, NO_LAYER);
    plan_error("local g = sprites.group({slots = 100})\nlocal h = sprites.group({slots = 30})\nfunction frame(t, f)\n  g:draw(dot, 1, 1)\n  h:draw(dot, 1, 1)\nend", 7, "sprites groups need 130 slots; 128 are free");
    // A group made in init, and one whose loop runs over a file-level table, size without slots.
    let mut e = LuaEngine::new();
    e.set_source(&format!("{SHEET}{KEYED}local XS = {{10, 20, 30}}\nlocal g\nfunction init() g = sprites.group() end\nfunction frame(t, f)\n  for _, x in ipairs(XS) do g:draw(dot, x, 9) end\n  sprites.draw(big, 99, 99)\nend")).unwrap();
    e.frame(0., 0).unwrap();
    let oam = &e.memory().oam;
    assert_eq!((oam[2].x, oam[3].x), (22, 83));
}

/// A group made and never drawn shows nothing and takes no slots (the cartridge keeps none either).
#[test]
fn a_group_never_drawn_is_inert() {
    let e = run_as("main.luau", "local idle = sprites.group()\nfunction frame(t, f) obj[0].x = f end", 3);
    assert_eq!(e.memory().oam[0].x, 2);
}

/// `group:draw(if c then a else b, ...)`: the group sizes for the larger of the two arts.
#[test]
fn an_if_expression_picks_the_art_of_a_draw() {
    let program = r#"local dot = sprites.art({{size = 16, columns = 1, cells = {1}}}, sheet)
local pair = sprites.art({{size = 16, columns = 2, cells = {2, 3}}}, sheet)
local g = sprites.group()
function frame(t, f)
  g:draw(if f % 2 == 0 then dot else pair, 100, 100)
end"#;
    let e = run_as("main.luau", program, 2);
    let on: Vec<u16> = e.memory().oam.iter().filter(|o| o.on).map(|o| o.tile).collect();
    assert_eq!(on, vec![4, 6]);
    let e = run_as("main.luau", program, 1);
    let on: Vec<u16> = e.memory().oam.iter().filter(|o| o.on).map(|o| o.tile).collect();
    assert_eq!(on, vec![2]);
}

/// Block sheets as dma() returns them: `name` with n cells of w x h, every tile drawn (`opaque`).
const BLOCKS: &str = r#"local function blocks(w, h, n, pal)
  local cells = {}
  for i = 1, n do
    local rows = {}
    for r = 1, h // 8 do rows[r] = 2 ^ (w // 8) - 1 end
    cells[i] = {tile = (i - 1) * (w // 8), pal = 0, opaque = rows}
  end
  return {tile = 0, pal = pal, cell_w = w, cell_h = h, cells = cells}
end
"#;

fn sized(program: &str, frames: u32) -> Result<LuaEngine, ppu_core::LuaError> {
    let mut engine = LuaEngine::new();
    engine.set_sources(&[("main.luau", &format!("{BLOCKS}{program}"))])?;
    for f in 0..frames {
        engine.frame(f as f64 / 60., f)?;
    }
    Ok(engine)
}

fn sprite(e: &LuaEngine, i: usize) -> (i32, i32, u16, u8, bool, bool, bool, bool) {
    let o = &e.memory().oam[i];
    (o.x.into(), o.y.into(), o.tile, o.pal, o.large, o.flip_x, o.flip_y, o.on)
}

#[test]
fn the_size_pair_holds_the_arts_sizes_and_draws_flip_and_recolour() {
    let e = sized(
        r#"local small = {tile = 100, pal = 1, cell_w = 8, cell_h = 8, cells = {{tile = 0, pal = 0}, {tile = 1, pal = 2, flip_x = true}}}
local shot = sprites.art({{size = 8, columns = 1, cells = {1}}}, small)
local plane = sprites.art({{size = 32, columns = 1, cells = {1}}}, blocks(32, 32, 2, 4))
function frame()
  sprites.draw(shot, 100, 100)
  sprites.draw(shot, 100, 100, {flip_x = true, flip_y = true})
  sprites.draw(plane, 50, 60, {pal = 6, flip_y = true, prio = 1})
  sprites.draw(plane, 50, 60)
end"#,
        1,
    )
    .unwrap();
    // 8x8 and 32x32 cells: OBJSEL pair 1.
    assert_eq!(e.memory().obsel.size_sel, 1);
    // Cell 1's own palette (the sheet's 1 + its 2) and its importer flip, which a draw's flip undoes.
    assert_eq!(sprite(&e, 0), (96, 96, 101, 3, false, true, false, true));
    assert_eq!(sprite(&e, 1), (96, 96, 101, 3, false, false, true, true));
    assert_eq!(sprite(&e, 2), (34, 44, 4, 6, true, false, true, true));
    assert_eq!(sprite(&e, 3), (34, 44, 4, 4, true, false, false, true));
    assert_eq!(e.memory().oam[2].prio, 1);
}

#[test]
fn a_composite_cell_is_its_fewest_pieces_mirrored_as_one() {
    // 16x16 art fixes pair 0, 3 or 4; only pair 0 builds 24x24 (one 16x16 and five 8x8); its last tile is blank.
    let program = r#"local dot = sprites.art({{size = 16, columns = 1, cells = {0}}}, blocks(16, 16, 1, 0))
local s = blocks(24, 24, 2, 0)
s.cells[1].opaque[3] = 3
local ship = sprites.art({{size = 24, columns = 1, cells = {0}}}, s)
local g = sprites.group()
function frame(t, f)
  g:draw(ship, 100, 100, {flip_x = f == 1, key = 1})
  sprites.draw(dot, 8, 8)
end"#;
    let e = sized(program, 1).unwrap();
    assert_eq!(e.memory().obsel.size_sel, 0);
    let pieces: Vec<_> = (0..6).map(|i| sprite(&e, i)).collect();
    assert_eq!(
        pieces,
        vec![
            (88, 88, 0, 0, true, false, false, true),
            (104, 88, 2, 0, false, false, false, true),
            (104, 96, 18, 0, false, false, false, true),
            (88, 104, 32, 0, false, false, false, true),
            (96, 104, 33, 0, false, false, false, true),
            (0, 0, 0, 0, true, false, false, true),
        ]
    );
    // The keyed draw flipped: each piece in its slot, mirrored around x = 100 and flipped.
    let e = sized(program, 2).unwrap();
    let pieces: Vec<_> = (0..5).map(|i| sprite(&e, i)).collect();
    assert_eq!(
        pieces,
        vec![
            (96, 88, 0, 0, true, true, false, true),
            (88, 88, 2, 0, false, true, false, true),
            (88, 96, 18, 0, false, true, false, true),
            (104, 104, 32, 0, false, true, false, true),
            (96, 104, 33, 0, false, true, false, true),
        ]
    );
}

#[test]
fn a_pinned_pair_builds_the_rest_and_too_many_sizes_is_an_error() {
    // The program's own pair: 16x32 cells are its small sprites, 32x32 cells two of them.
    let e = sized(
        r#"local tall = sprites.art({{size = 32, columns = 1, cells = {0}}}, blocks(16, 32, 1, 0))
local wide = sprites.art({{size = 32, columns = 1, cells = {0}}}, blocks(32, 64, 1, 0))
function init() obj.size_sel = 7 end
function frame()
  sprites.draw(tall, 100, 100)
  sprites.draw(wide, 100, 100)
end"#,
        1,
    )
    .unwrap();
    assert_eq!(e.memory().obsel.size_sel, 7);
    assert_eq!(sprite(&e, 0), (92, 84, 0, 0, false, false, false, true));
    assert_eq!((sprite(&e, 1), sprite(&e, 2)), ((84, 68, 0, 0, true, false, false, true), (84, 100, 64, 0, true, false, false, true)));
    let three = r#"local a = sprites.art({{size = 8, columns = 1, cells = {0}}}, blocks(8, 8, 1, 0))
local b = sprites.art({{size = 16, columns = 1, cells = {0}}}, blocks(16, 16, 1, 0))
local c = sprites.art({{size = 32, columns = 1, cells = {0}}}, blocks(32, 32, 1, 0))
function frame() sprites.draw(a, 100, 100) sprites.draw(b, 100, 100) sprites.draw(c, 100, 100) end"#;
    let err = sized(three, 1).err().unwrap();
    assert_eq!(err.line, Some(BLOCKS.lines().count() as u32 + 3));
    assert!(err.message.contains("8x8, 16x16, 32x32") && err.message.contains("obj.size_sel"), "{}", err.message);
    // Pinned to pair 1, the 16x16 art is four 8x8 sprites.
    let e = sized(&format!("{three}\nfunction init() obj.size_sel = 1 end"), 1).unwrap();
    assert_eq!((sprite(&e, 1).4, sprite(&e, 4).4, sprite(&e, 5).4), (false, false, true));
    // Pair 3 has no 8x8 sprite.
    let err = sized(&format!("{three}\nfunction init() obj.size_sel = 3 end"), 1).err().unwrap();
    assert!(err.message.contains("8x8 can't be built"), "{}", err.message);
}

#[test]
fn a_64_pixel_tall_sprite_shows_only_where_it_cannot_wrap() {
    // Drawn at its center: the piece's y is 32 above. It shows from -32 (y 0) to 192 (y 224).
    for (y, on) in [(0, true), (-1, false), (224, true), (225, false)] {
        let program = format!(
            "local big = sprites.art({{{{size = 64, columns = 1, cells = {{0}}}}}}, blocks(64, 64, 1, 0))\nfunction frame() sprites.draw(big, 100, {y}) end"
        );
        assert_eq!(sprite(&sized(&program, 1).unwrap(), 0).7, on, "y {y}");
    }
}
