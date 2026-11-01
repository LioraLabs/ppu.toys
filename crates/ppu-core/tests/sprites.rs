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
    };
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
