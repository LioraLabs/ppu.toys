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
    engine
        .set_sources(&[(file, &format!("{SHEET}{program}"))])
        .unwrap();
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
    assert_eq!(
        (oam[0].x, oam[1].x, oam[1].tile, oam[24].x, oam[25].x),
        (42, 24, 8, 120, -3)
    );
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
    let e = run(
        r#"local dot = sprites.art({{size = 16, columns = 1, cells = {1}}}, sheet)
function frame(t, f)
  obj[0].x, obj[0].on = 77, true
  for i = 1, 140 do sprites.draw(dot, i, 50) end
end"#,
        1,
    );
    let oam = &e.memory().oam;
    assert_eq!((oam[0].x, oam[0].on), (77, true));
    // Draw order; the default group's 127 slots drop draws 128 and later.
    assert_eq!((oam[1].x, oam[127].x), (-7, 119));
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
    assert!(
        err.message
            .contains("sprites.draw and sprites.hold cannot both run in one frame"),
        "{}",
        err.message
    );
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
    assert_eq!(
        o[1..5]
            .iter()
            .map(|s| (s.x, s.y, s.tile))
            .collect::<Vec<_>>(),
        [(84, 84, 8), (100, 84, 10), (84, 100, 12), (100, 100, 14)]
    );
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
    keyed_error(
        "  g:draw(dot, 1, 1, {key = 5})\n  if f == 2 then g:draw(dot, 2, 2, {key = 5}) end",
        "sprites key 5 is drawn twice in one frame",
    );
    keyed_error("  if f < 2 then g:draw(dot, 1, 1, {key = 5}) else g:keep(5) g:draw(dot, 1, 1, {key = 5}) end", "both drawn and kept");
    keyed_error(
        "  g:draw(dot, 1, 1, {key = 5})\n  if f == 2 then g:keep(5) end",
        "both drawn and kept",
    );
    keyed_error("  if f == 0 then g:draw(dot, 1, 1, {key = 5}) else g:keep(5) end\n  if f == 2 then g:keep(5) end", "kept twice");
    keyed_error(
        "  if f == 2 then g:keep(6) end",
        "keep(6): the key was never drawn in this group",
    );
    // Keys are the group's: key 5 drawn in g is not h's.
    keyed_error(
        "  g:draw(dot, 1, 1, {key = 5})\n  if f == 2 then h:keep(5) end",
        "keep(5): the key was never drawn in this group",
    );
    keyed_error(
        "  g:draw(f == 2 and big or dot, 1, 1, {key = 5})",
        "sprites key 5 has 1 slots, from its first art; this art needs 4",
    );
    keyed_error(
        "  g:draw(dot, 1, 1, {key = f * 64})",
        "draw key must be an integer 0..127",
    );
    keyed_error(
        "  if f == 2 then for k = 0, 32 do g:draw(big, 1, 1, {key = k}) end end",
        "sprites key 32 needs 4 slots; 0 are left in its group",
    );
}

fn plan_error(program: &str, line: u32, message: &str) {
    let err = LuaEngine::new()
        .set_source(&format!("{SHEET}{KEYED}{program}"))
        .unwrap_err();
    assert_eq!(
        (err.message.as_str(), err.line),
        (format!("error[snes.sprites]: {message}").as_str(), Some(line)),
        "{program}"
    );
}

#[test]
fn group_rules_are_static_errors_on_their_line() {
    use ppu_core::sprites::{GROUP_PLACE, NO_GROUP, NO_LAYER, NO_SORT, UNSIZED};
    // SHEET is lines 1-3, KEYED 4-5: the program starts on line 6.
    plan_error(
        "function frame(t, f)\n  local g = sprites.group()\n  g:draw(dot, 1, 1)\nend",
        7,
        GROUP_PLACE,
    );
    plan_error("local g = sprites.group()\nfunction frame(t, f)\n  while f > 0 do g:draw(dot, 1, 1) f = f - 1 end\nend", 8, UNSIZED);
    // `c and a or b` (Luau's `if c then a else b` as the preview runs it) sizes for either art; a value
    // that is not art does not.
    plan_error("local g = sprites.group()\nlocal other = {}\nfunction frame(t, f)\n  g:draw(f > 1 and dot or other, 1, 1)\nend", 9, UNSIZED);
    plan_error(
        "function frame(t, f, g)\n  g:draw(dot, 1, 1)\nend",
        7,
        NO_GROUP,
    );
    plan_error(
        "function frame(t, f)\n  sprites.sort()\n  sprites.draw(dot, 1, 1)\nend",
        7,
        NO_SORT,
    );
    plan_error(
        "function frame(t, f)\n  sprites.draw(dot, 1, 1, {layer = 2})\nend",
        7,
        NO_LAYER,
    );
    plan_error("local g = sprites.group({slots = 100})\nlocal h = sprites.group({slots = 30})\nfunction frame(t, f)\n  g:draw(dot, 1, 1)\n  h:draw(dot, 1, 1)\nend", 7, "the sprites group at source:7 needs 30 slots in a row and 28 are left: the sprites group at source:6 takes 100 slots; make fewer obj[] writes or smaller groups");
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
    let e = run_as(
        "main.luau",
        "local idle = sprites.group()\nfunction frame(t, f) obj[0].x = f end",
        3,
    );
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
    let on: Vec<u16> = e
        .memory()
        .oam
        .iter()
        .filter(|o| o.on)
        .map(|o| o.tile)
        .collect();
    assert_eq!(on, vec![4, 6]);
    let e = run_as("main.luau", program, 1);
    let on: Vec<u16> = e
        .memory()
        .oam
        .iter()
        .filter(|o| o.on)
        .map(|o| o.tile)
        .collect();
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
    (
        o.x.into(),
        o.y.into(),
        o.tile,
        o.pal,
        o.large,
        o.flip_x,
        o.flip_y,
        o.on,
    )
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
    assert_eq!(
        (sprite(&e, 1), sprite(&e, 2)),
        (
            (84, 68, 0, 0, true, false, false, true),
            (84, 100, 64, 0, true, false, false, true)
        )
    );
    let three = r#"local a = sprites.art({{size = 8, columns = 1, cells = {0}}}, blocks(8, 8, 1, 0))
local b = sprites.art({{size = 16, columns = 1, cells = {0}}}, blocks(16, 16, 1, 0))
local c = sprites.art({{size = 32, columns = 1, cells = {0}}}, blocks(32, 32, 1, 0))
function frame() sprites.draw(a, 100, 100) sprites.draw(b, 100, 100) sprites.draw(c, 100, 100) end"#;
    let err = sized(three, 1).err().unwrap();
    assert_eq!(err.line, Some(BLOCKS.lines().count() as u32 + 3));
    assert!(
        err.message.contains("8x8, 16x16, 32x32") && err.message.contains("obj.size_sel"),
        "{}",
        err.message
    );
    // Pinned to pair 1, the 16x16 art is four 8x8 sprites.
    let e = sized(&format!("{three}\nfunction init() obj.size_sel = 1 end"), 1).unwrap();
    assert_eq!(
        (sprite(&e, 1).4, sprite(&e, 4).4, sprite(&e, 5).4),
        (false, false, true)
    );
    // Pair 3 has no 8x8 sprite.
    let err = sized(&format!("{three}\nfunction init() obj.size_sel = 3 end"), 1)
        .err()
        .unwrap();
    assert!(
        err.message.contains("8x8 can't be built"),
        "{}",
        err.message
    );
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

#[test]
fn groups_set_up_obsel_and_the_sprite_layer_the_program_leaves() {
    // A sheet as dma("hero", {char = 0x6000}) returns it: its tiles measured from char 0x6000.
    let hero = "local hero = {char = 0x6000, tile = 0, pal = 0, cells = {{tile = 0}}}\n";
    let body = "local idle = sprites.art({{size = 16, columns = 1, cells = {0}}}, hero)\nlocal actors = sprites.group()\n";
    let frame = "function frame(t, f) actors:draw(idle, 128, 104) end\n";
    let setup = |e: &mut LuaEngine| {
        let lt = e.frame(0.0, 0).unwrap();
        (e.memory().obsel.char_base, lt.rows[104].tm & 0x10 != 0)
    };
    let mut e = LuaEngine::new();
    e.set_sources(&[("main.luau", &format!("{hero}{body}{frame}"))])
        .unwrap();
    assert_eq!(
        setup(&mut e),
        (0x6000, true),
        "the library sets what the program leaves"
    );
    // A program that sets either keeps it as it wrote it.
    for (init, want) in [
        (
            "function init() obj.char_base = 0x2000 end\n",
            (0x2000, true),
        ),
        (
            "function init() screen.sub.obj = true end\n",
            (0x6000, false),
        ),
        (
            "function init() obj.char_base, screen.main.obj = 0x6000, false end\n",
            (0x6000, false),
        ),
    ] {
        let mut e = LuaEngine::new();
        e.set_sources(&[("main.luau", &format!("{hero}{body}{init}{frame}"))])
            .unwrap();
        assert_eq!(setup(&mut e), want, "{init}");
    }
    // Sheets placed from two OBSEL bases: the library can't hold both.
    let two = "local hero = {char = 0x6000, tile = 0, pal = 0, cells = {{tile = 0}}}\nlocal foe = {char = 0x4000, tile = 0, pal = 0, cells = {{tile = 0}}}\n";
    let art = "local a = sprites.art({{size = 16, columns = 1, cells = {0}}}, hero)\nlocal b = sprites.art({{size = 16, columns = 1, cells = {0}}}, foe)\n";
    let draws = "function frame() sprites.draw(a, 10, 10) sprites.draw(b, 30, 10) end\n";
    let err = LuaEngine::new()
        .set_sources(&[("main.luau", &format!("{two}{art}{draws}"))])
        .err()
        .unwrap();
    assert_eq!(err.line, Some(4));
    assert!(
        err.message.contains("0x6000 and 0x4000")
            && err.message.contains("set obj.char_base yourself"),
        "{}",
        err.message
    );
    let mut e = LuaEngine::new();
    e.set_sources(&[(
        "main.luau",
        &format!("{two}{art}function init() obj.char_base = 0x4000 end\n{draws}"),
    )])
    .unwrap();
    assert_eq!(
        setup(&mut e).0,
        0x4000,
        "a program that sets it draws as it chose"
    );
}

/// The tile of slot 0 over `frames` frames: cell n is tile 2n.
fn tiles(program: &str, frames: u32) -> Vec<u16> {
    let mut engine = LuaEngine::new();
    engine.set_sources(&[("source", &format!("{SHEET}{program}"))]).unwrap();
    (0..frames)
        .map(|f| {
            engine.frame(f as f64 / 60., f).unwrap();
            engine.memory().oam.iter().find(|o| o.on).map_or(0, |o| o.tile)
        })
        .collect()
}

#[test]
fn an_animation_plays_on_its_keys_clock_and_restarts_when_the_key_switches() {
    // Cells 1, 2, 3 two frames each; frame 5 draws the still art (cell 9), frame 6 the walk again from its start.
    let t = tiles(
        r#"local walk = sprites.animation({
  sprites.art({{size = 16, columns = 1, cells = {1}}}, sheet),
  sprites.art({{size = 16, columns = 1, cells = {2}}}, sheet),
  sprites.art({{size = 16, columns = 1, cells = {3}}}, sheet),
}, 2)
local still = sprites.art({{size = 16, columns = 1, cells = {9}}}, sheet)
local g = sprites.group()
function frame(t, f)
  g:draw(if f == 5 then still else walk, 50, 50, {key = 0})
end"#,
        10,
    );
    assert_eq!(t, [2, 2, 4, 4, 6, 18, 2, 2, 4, 4]);
}

#[test]
fn a_paused_repeat_holds_a_keys_animation_clock() {
    // One frame a cell: frames 0, 1, then frame 2 five times (a paused Studio), then 3.
    let mut engine = LuaEngine::new();
    let program = r#"local walk = sprites.animation({
  sprites.art({{size = 16, columns = 1, cells = {1}}}, sheet),
  sprites.art({{size = 16, columns = 1, cells = {2}}}, sheet),
  sprites.art({{size = 16, columns = 1, cells = {3}}}, sheet),
  sprites.art({{size = 16, columns = 1, cells = {4}}}, sheet),
}, 1)
local g = sprites.group()
function frame(t, f)
  g:draw(walk, 50, 50, {key = 0})
end"#;
    engine.set_sources(&[("source", &format!("{SHEET}{program}"))]).unwrap();
    let t: Vec<u16> = [0, 1, 2, 2, 2, 2, 2, 3]
        .iter()
        .map(|&f| {
            engine.frame(f as f64 / 60., f).unwrap();
            engine.memory().oam.iter().find(|o| o.on).map_or(0, |o| o.tile)
        })
        .collect();
    assert_eq!(t, [2, 4, 6, 6, 6, 6, 6, 8]);
}

#[test]
fn a_keys_animation_clock_restarts_at_a_scenes_entry() {
    // Scene A draws the walk on key 0 for updates 0-2 and switches on 2; B's first update draws it from its first art.
    let shared = format!(
        "{SHEET}local walk = sprites.animation({{\n  sprites.art({{{{size = 16, columns = 1, cells = {{1}}}}}}, sheet),\n  sprites.art({{{{size = 16, columns = 1, cells = {{2}}}}}}, sheet),\n  sprites.art({{{{size = 16, columns = 1, cells = {{3}}}}}}, sheet),\n  sprites.art({{{{size = 16, columns = 1, cells = {{4}}}}}}, sheet),\n  sprites.art({{{{size = 16, columns = 1, cells = {{5}}}}}}, sheet),\n}}, 1)\nreturn {{walk = walk, g = sprites.group()}}\n"
    );
    let a = "local S = require(\"./shared\")\nlocal function frame(t, f)\n  S.g:draw(S.walk, 50, 50, {key = 0})\n  if f == 2 then scenes.go(B) end\nend\nreturn {frame = frame}\n";
    let b = "local S = require(\"./shared\")\nlocal function frame(t, f)\n  S.g:draw(S.walk, 50, 50, {key = 0})\nend\nreturn {frame = frame}\n";
    let main = "scenes.add(require(\"./a\"))\nB = scenes.add(require(\"./b\"))\n";
    let mut engine = LuaEngine::new();
    engine.set_sources(&[("main.luau", main), ("shared.luau", &shared), ("a.luau", a), ("b.luau", b)]).unwrap();
    let t: Vec<u16> = (0..6)
        .map(|f| {
            engine.frame(f as f64 / 60., f).unwrap();
            engine.memory().oam.iter().find(|o| o.on).map_or(0, |o| o.tile)
        })
        .collect();
    assert_eq!(t, [2, 4, 6, 2, 4, 6]);
}

#[test]
fn an_animation_once_holds_its_last_art_and_at_sets_its_clock() {
    let t = tiles(
        r#"local A = {
  sprites.art({{size = 16, columns = 1, cells = {1}}}, sheet),
  sprites.art({{size = 16, columns = 1, cells = {2}}}, sheet),
}
local pop = sprites.animation(A, 3, {once = true})
local spin = sprites.animation(A, 1)
local g = sprites.group({slots = 2})
function frame(t, f)
  if f < 4 then g:draw(pop, 50, 50, {key = 1}) else g:draw(spin, 50, 50, {at = f}) end
end"#,
        8,
    );
    // Once: 3 frames of cell 1, then cell 2 held; at = f: cell (f % 2) + 1.
    assert_eq!(t, [2, 2, 2, 4, 2, 4, 2, 4]);
}

#[test]
fn an_unkeyed_draw_of_an_animation_needs_at() {
    let mut engine = LuaEngine::new();
    engine
        .set_sources(&[(
            "source",
            &format!(
                "{SHEET}local a = sprites.animation({{sprites.art({{{{size = 16, columns = 1, cells = {{1}}}}}}, sheet)}}, 4)\nfunction frame(t, f)\n  sprites.draw(a, 1, 1)\nend"
            ),
        )])
        .unwrap();
    let e = engine.frame(0., 0).unwrap_err();
    assert!(e.message.contains("needs a key, whose clock it plays on, or {at = frames}"), "{}", e.message);
}

#[test]
fn an_animations_arts_take_their_own_durations_in_video_frames_also_at_30_hz() {
    let program = r#"local walk = sprites.animation({
  sprites.art({{size = 16, columns = 1, cells = {1}}}, sheet),
  sprites.art({{size = 16, columns = 1, cells = {2}}}, sheet),
}, {1, 3})
local g = sprites.group()
function frame(t, f)
  g:draw(walk, 50, 50, {key = 0})
end"#;
    assert_eq!(tiles(program, 8), [2, 4, 4, 4, 2, 4, 4, 4]);
    // At 30 Hz an update spans two video frames: the clock goes 0, 2, 4 (0 again), 6 (2)...
    let mut engine = LuaEngine::new();
    engine.set_target_hz(Some(30)).unwrap();
    engine.set_sources(&[("source", &format!("{SHEET}{program}"))]).unwrap();
    let t: Vec<u16> = (0..8)
        .map(|f| {
            engine.frame(f as f64 / 60., f).unwrap();
            engine.memory().oam.iter().find(|o| o.on).map_or(0, |o| o.tile)
        })
        .collect();
    assert_eq!(t, [2, 2, 4, 4, 2, 2, 4, 4]);
}

/// Art for the plan tests: a module of art in nested tables, and an animation that names its arts through them.
const HERO: &str = r#"local sheet = dma("ships", {char = 0x4000, pal = 0})
local HERO = {
  down = {sprites.art({{size = 16, columns = 1, cells = {0}}}, sheet), sprites.art({{size = 16, columns = 1, cells = {1}}}, sheet)},
  still = sprites.art({{size = 16, columns = 1, cells = {2}}}, sheet),
}
local walk = sprites.animation({HERO.down[1], HERO.down[2]}, 8)
return {HERO = HERO, walk = walk, group = sprites.group({slots = 4})}
"#;

fn plan(main: &str) -> ppu_core::sprites::Plan {
    ppu_core::sprites::plan(&[("shared.luau", HERO), ("main.luau", main)]).unwrap().unwrap()
}

#[test]
fn an_animation_names_its_arts_through_nested_tables_and_modules() {
    let p = plan("local S = require(\"./shared\")\nlocal run = sprites.animation({S.HERO.down[2], S.HERO.still}, 4)\nfunction frame(t, f)\n  S.group:draw(run, 1, 1, {at = f})\nend\n");
    assert_eq!(p.animations.iter().map(|a| a.arts.clone()).collect::<Vec<_>>(), [vec![1, 2], vec![2, 3]]);
    assert_eq!(p.calls[0].animations, Some(vec![2]));
}

#[test]
fn art_no_animation_can_reach_draws_as_plain_art() {
    // `pick` returns plain art the plan can't name; `show` is handed the walk, so its draw can show an animation.
    let p = plan(
        "local S = require(\"./shared\")\nlocal function pick(f) return if f % 2 == 0 then S.HERO.still else S.HERO.down[1] end\nlocal function show(art, f)\n  S.group:draw(art, 2, 2, {at = f})\nend\nfunction frame(t, f)\n  S.group:draw(pick(f), 1, 1)\n  show(S.walk, f)\nend\n",
    );
    assert!(p.escapes.is_empty(), "{:?}", p.escapes);
    let by_line = |l: u32| p.calls.iter().find(|c| c.line == l).unwrap().animations.clone();
    assert_eq!(by_line(7), Some(vec![]), "pick() never returns an animation");
    assert_eq!(by_line(4), None, "show's art can be the walk");
}

#[test]
fn an_animation_stored_where_the_plan_loses_it_is_an_escape() {
    for (code, line) in [
        ("local S = require(\"./shared\")\nlocal now = {}\nfunction frame(t, f)\n  now[1] = S.walk\n  S.group:draw(now[1], 1, 1, {at = f})\nend\n", 4),
        ("local S = require(\"./shared\")\nlocal function get() return S.walk end\nfunction frame(t, f)\n  S.group:draw(get(), 1, 1, {at = f})\nend\n", 2),
        ("local S = require(\"./shared\")\nlocal list = {}\nfunction frame(t, f)\n  table.insert(list, S.walk)\n  S.group:draw(S.HERO.still, 1, 1)\nend\n", 4),
        ("local S = require(\"./shared\")\nlocal function show(...)\n  S.group:draw(S.HERO.still, 1, 1)\nend\nfunction frame(t, f)\n  show(S.walk)\nend\n", 6),
    ] {
        let p = plan(code);
        assert_eq!(p.escapes.iter().map(|e| (e.file.as_str(), e.line)).collect::<Vec<_>>(), [("main.luau", line)], "{code}");
    }
}

/// The draw at `file`:`line` can show the walk: it keeps the machinery (`animations: None`), or the plan refuses it.
fn sound(files: &[(&str, &str)], file: &str, line: u32) -> bool {
    // A plan that refuses the program is sound too.
    let Ok(p) = ppu_core::sprites::plan(files) else { return true };
    let p = p.unwrap();
    let draw = p.calls.iter().find(|c| c.file == file && c.line == line).unwrap_or_else(|| panic!("no draw at {file}:{line}"));
    !p.escapes.is_empty() || draw.animations.is_none()
}

#[test]
fn every_shape_an_animation_reaches_a_draw_through_keeps_it_or_is_refused() {
    // a. A main that never names sprites wires the modules: Player.show(S.walk, f).
    let player = "local S = require(\"./shared\")\nlocal function show(a, f)\n  S.group:draw(a, 1, 1, {at = f})\nend\nreturn {show = show}\n";
    let main = "local S = require(\"./shared\")\nlocal P = require(\"./player\")\nfunction frame(t, f)\n  P.show(S.walk, f)\nend\n";
    let mut unsound = vec![];
    if !sound(&[("shared.luau", HERO), ("player.luau", player), ("main.luau", main)], "player.luau", 3) {
        unsound.push("a main wiring modules");
    }
    let main = |body: &str| format!("local S = require(\"./shared\")\nlocal function show(o)\n  S.group:draw(o.art, 1, 1, {{at = o.f}})\nend\nlocal function shown(a, f)\n  S.group:draw(a, 2, 2, {{at = f}})\nend\nlocal A = {{S.walk}}\nfunction frame(t, f)\n{body}\nend\n");
    for (body, line, what) in [
        // b. A table-call argument.
        ("  show{art = S.walk, f = f}", 3, "show{...}"),
        // c. Type assertions, passed on and drawn from a local.
        ("  shown(S.walk :: any, f)", 6, "an asserted argument"),
        ("  local a = S.walk :: any\n  S.group:draw(a, 3, 3, {at = f})", 11, "an asserted local"),
        // d. A parenthesised prefix.
        ("  S.group:draw((A)[1], 4, 4, {at = f})", 10, "(A)[1]"),
    ] {
        if !sound(&[("shared.luau", HERO), ("main.luau", &main(body))], "main.luau", line) {
            unsound.push(what);
        }
    }
    // e. A module's returned table holding a named animation beside a leaf its tree can't name.
    let main = "local S = require(\"./shared\")\nlocal A = require(\"./actor\")\nfunction frame(t, f)\n  S.group:draw(A.ANIMS.walk, 1, 1, {at = f})\nend\n";
    let actor = |body: &str| format!("local S = require(\"./shared\")\nlocal hop = sprites.animation({{S.HERO.down[1], S.HERO.still}}, 5)\nlocal hop2 = sprites.animation({{S.HERO.still, S.HERO.down[2]}}, 3)\nlocal FAST = true\n{body}\n");
    for (body, what) in [
        ("return {ANIMS = {hop = hop, walk = S.walk}}", "a re-exported field"),
        ("local run = if FAST then hop2 else hop\nreturn {ANIMS = {hop = hop, walk = run}}", "an if-expression"),
        ("return {ANIMS = {hop = hop, walk = (if FAST then hop2 else hop)}}", "an inline if-expression"),
        ("local ANIMS = {hop = hop, walk = S.walk}\nreturn {ANIMS = ANIMS}", "a named table with a re-exported field"),
        ("return {ANIMS = {hop = hop, more = {walk = S.walk}}}", "a nested re-export"),
        ("return {ANIMS = {hop, S.walk}, hop = hop}", "a positional re-export"),
        ("local inner = {walk = S.walk}\nlocal ANIMS = {inner = inner, hop = hop, walk = S.walk}\nreturn {ANIMS = ANIMS}", "a table of tables"),
        ("local ANIMS = {hop = hop}\nANIMS.walk = S.walk\nreturn {ANIMS = ANIMS}", "a field assigned later"),
        ("return {ANIMS = {hop = hop, [\"walk\"] = S.walk}}", "a bracketed key"),
    ] {
        let files = [("shared.luau", HERO), ("actor.luau", &actor(body)), ("main.luau", &main.replace("ANIMS.walk", if what.contains("nested") { "ANIMS.more.walk" } else if what.contains("positional") { "ANIMS[2]" } else { "ANIMS.walk" }))];
        if !sound(&files, "main.luau", 4) {
            unsound.push(what);
        }
    }
    // f. A file-level name that starts as still art and is assigned an animation later, in its own file or exported.
    let art = "local sheet = dma(\"ships\", {char = 0x4000, pal = 0})\nlocal one = sprites.art({{size = 16, columns = 1, cells = {0}}}, sheet)\nlocal two = sprites.art({{size = 16, columns = 1, cells = {1}}}, sheet)\nlocal hop = sprites.animation({one, two}, 3)\nlocal FANCY = true\n";
    let drawn = |src: &str| src.lines().position(|l| l.contains(":draw(")).unwrap() as u32 + 1;
    for (body, what) in [
        ("local cur = one\nfunction frame(t, f)\n  if f % 10 < 5 then cur = hop else cur = one end\n  g:draw(cur, 60, 80, {key = 1})\nend\n", "a state machine in frame"),
        ("local cur = one\nfunction frame(t, f)\n  cur = if f > 1 then hop else one\n  g:draw(cur, 60, 80, {at = f})\nend\n", "an if-expression in frame"),
        ("local cur = one\nif FANCY then cur = hop end\nfunction frame(t, f)\n  g:draw(cur, 60, 80, {at = f})\nend\n", "a reassignment at file level"),
        ("local w = one\nw = hop\nlocal ANIMS = {walk = w}\nfunction frame(t, f)\n  g:draw(ANIMS.walk, 60, 80, {at = f})\nend\n", "a table of a reassigned local"),
        ("local ANIMS = {walk = one}\nif FANCY then ANIMS = {walk = hop} end\nfunction frame(t, f)\n  g:draw(ANIMS.walk, 60, 80, {at = f})\nend\n", "a reassigned table"),
    ] {
        let main = format!("{art}local g = sprites.group({{slots = 4}})\n{body}");
        if !sound(&[("main.luau", &main)], "main.luau", drawn(&main)) {
            unsound.push(what);
        }
    }
    let main = "local H = require(\"./art\")\nfunction frame(t, f)\n  H.group:draw(H.current, 60, 80, {at = f})\n  H.group:draw(H.ANIMS.walk, 90, 80, {at = f})\nend\n";
    for (tail, line, what) in [
        ("local current = one\nif FANCY then current = hop end\nreturn {current = current, ANIMS = {}, group = sprites.group({slots = 4})}\n", 3, "an exported reassigned local"),
        ("local current = one\nlocal function setup() current = hop end\nsetup()\nreturn {current = current, ANIMS = {}, group = sprites.group({slots = 4})}\n", 3, "a local reassigned in a load-time setup()"),
        ("local ANIMS = {walk = one}\nif FANCY then ANIMS = {walk = hop} end\nreturn {current = one, ANIMS = ANIMS, group = sprites.group({slots = 4})}\n", 4, "an exported reassigned table"),
        ("local w = one\nw = hop\nlocal ANIMS = {walk = w}\nreturn {current = one, ANIMS = ANIMS, group = sprites.group({slots = 4})}\n", 4, "an exported table of a reassigned local"),
        ("local w = one\nw = hop\nreturn {current = one, ANIMS = {walk = w}, group = sprites.group({slots = 4})}\n", 4, "an exported inline table of a reassigned local"),
    ] {
        if !sound(&[("art.luau", &format!("{art}{tail}")), ("main.luau", main)], "main.luau", line) {
            unsound.push(what);
        }
    }
    // g. A file-level still-art name declared again in an inner scope (local, parameter, loop) that holds an animation:
    // its draw keeps the machinery, or names the animation the inner name holds, never the file-level still.
    let shows = |files: &[(&str, &str)], line: u32| {
        sound(files, "main.luau", line)
            || ppu_core::sprites::plan(files).unwrap().unwrap().calls.iter().any(|c| c.line == line && c.animations.as_ref().is_some_and(|a| !a.is_empty()))
    };
    for (body, what) in [
        ("local cur = one\nfunction frame(t, f)\n  local cur = hop\n  g:draw(cur, 60, 80, {at = f})\nend\n", "a local in frame, at"),
        ("local cur = one\nfunction frame(t, f)\n  local cur = hop\n  g:draw(cur, 60, 80, {key = 1})\nend\n", "a local in frame, key"),
        ("local cur = one\nlocal function d(cur, f)\n  g:draw(cur, 60, 80, {at = f})\nend\nfunction frame(t, f)\n  d(hop, f)\nend\n", "a parameter, at"),
        ("local anim = one\nlocal function show(anim)\n  g:draw(anim, 60, 80, {key = 1})\nend\nfunction frame(t, f)\n  show(hop)\nend\n", "a parameter, key"),
        ("local cur = one\nfunction frame(t, f)\n  for i = 1, 1 do\n    local cur = hop\n    g:draw(cur, 60, 80, {at = f})\n  end\nend\n", "a local in a for body"),
        ("local ANIMS = {walk = one}\nfunction frame(t, f)\n  local ANIMS = {walk = hop}\n  g:draw(ANIMS.walk, 60, 80, {at = f})\nend\n", "a shadowing table in frame"),
        // Shapes of my own: a nested function's parameter, a repeat body, a generic for's name, a local function's.
        ("local cur = one\nfunction frame(t, f)\n  local function d(cur)\n    g:draw(cur, 60, 80, {at = f})\n  end\n  d(hop)\nend\n", "a nested function's parameter"),
        ("local cur = one\nfunction frame(t, f)\n  repeat\n    local cur = hop\n    g:draw(cur, 60, 80, {at = f})\n  until true\nend\n", "a local in a repeat body"),
        ("local cur = one\nlocal L = {hop}\nfunction frame(t, f)\n  for _, cur in L do\n    g:draw(cur, 60, 80, {at = f})\n  end\nend\n", "a generic for's name"),
        ("local cur = one\nfunction frame(t, f)\n  local cur = hop\n  sprites.draw(cur, 60, 80, {at = f})\nend\n", "sprites.draw of a shadowing local"),
    ] {
        let main = format!("{art}local g = sprites.group({{slots = 4}})\n{body}");
        let line = main.lines().position(|l| l.contains("draw(cur") || l.contains("draw(anim") || l.contains("draw(ANIMS")).unwrap() as u32 + 1;
        if !shows(&[("main.luau", &main)], line) {
            unsound.push(what);
        }
    }
    let module = format!("{art}return {{still = one, hop = hop, group = sprites.group({{slots = 4}})}}\n");
    for (main, line, what) in [
        ("local H = require(\"./art\")\nlocal cur = H.still\nfunction frame(t, f)\n  local cur = H.hop\n  H.group:draw(cur, 60, 80, {at = f})\nend\n", 5, "an importer's local shadowing its alias"),
        ("local H = require(\"./art\")\nlocal cur = H.still\nlocal function d(cur, f)\n  H.group:draw(cur, 60, 80, {at = f})\nend\nfunction frame(t, f)\n  d(H.hop, f)\nend\n", 4, "an importer's parameter shadowing its alias"),
    ] {
        if !shows(&[("art.luau", &module), ("main.luau", main)], line) {
            unsound.push(what);
        }
    }
    // h. A function-level local that is reassigned or reused: its draw keeps the machinery, or names what it holds.
    for (body, what) in [
        ("function frame(t, f)\n  local cur = one\n  if f % 10 < 5 then cur = hop end\n  g:draw(cur, 60, 80, {key = 1})\nend\n", "a local reassigned by an if"),
        ("function frame(t, f)\n  local cur = one\n  cur = if f > 1 then hop else one\n  g:draw(cur, 60, 80, {at = f})\nend\n", "a local reassigned by an if-expression"),
        ("function frame(t, f)\n  local cur = one\n  local i = 0\n  while i < 1 do cur = hop i += 1 end\n  g:draw(cur, 60, 80, {at = f})\nend\n", "a local reassigned in a while"),
        ("local L = {one, two}\nfunction frame(t, f)\n  local cur = L[1 + f % 2]\n  cur = hop\n  g:draw(cur, 60, 80, {at = f})\nend\n", "a local from a table index, then reassigned"),
        ("function frame(t, f)\n  local a, b = one, hop\n  a, b = b, a\n  g:draw(a, 60, 80, {at = f})\nend\n", "a swap"),
        ("local function a() local cur = one\n  g:draw(cur, 10, 10, {key = 2}) end\nlocal function b(cur, f) g:draw(cur, 60, 80, {at = f}) end\nfunction frame(t, f)\n  a()\n  b(hop, f)\nend\n", "a function local leaking into another's parameter"),
        ("local L = {hop}\nfunction frame(t, f)\n  do local cur = one end\n  for _, cur in L do g:draw(cur, 60, 80, {at = f}) end\nend\n", "a local leaking into a for name"),
        ("local cur = hop\ndo local cur = one end\nfunction frame(t, f)\n  g:draw(cur, 60, 80, {at = f})\nend\n", "a file-level animation redeclared as art in a do block"),
    ] {
        let main = format!("{art}local g = sprites.group({{slots = 4}})\n{body}");
        let line = main.lines().count() as u32 - main.lines().rev().position(|l| l.contains(":draw(")).unwrap() as u32;
        if !shows(&[("main.luau", &main)], line) {
            unsound.push(what);
        }
    }
    // i. A file-level module alias or module field declared again after a function that reads the first: the
    // function's draw reads the declaration it sees, not the file's last one of that name.
    let plain = "local sheet = dma(\"ships\", {char = 0x4000, pal = 0})\nreturn {hop = sprites.art({{size = 16, columns = 1, cells = {2}}}, sheet)}\n";
    for (main, what) in [
        ("local H = require(\"./art\")\nlocal W = H.hop\nlocal function d(f)\n  local x = W\n  H.group:draw(x, 60, 80, {at = f})\nend\nlocal W = H.still\nfunction frame(t, f)\n  d(f)\nend\n", "a module field declared again"),
        ("local H = require(\"./art\")\nlocal g = H.group\nlocal function d(f)\n  local x = H.hop\n  g:draw(x, 60, 80, {at = f})\nend\nlocal H = require(\"./plain\")\nfunction frame(t, f)\n  d(f)\nend\n", "a module alias declared again"),
    ] {
        if !shows(&[("art.luau", &module), ("plain.luau", plain), ("main.luau", main)], 5) {
            unsound.push(what);
        }
    }
    assert!(unsound.is_empty(), "the walk reaches a draw marked plain through: {unsound:?}");
}

#[test]
fn a_constant_local_names_its_art_whatever_else_shares_its_name() {
    // Each `cur` is its own declaration, never assigned: the draws name the art it holds, with no animation machinery.
    let main = format!(
        "{SHEET}local one = sprites.art({{{{size = 16, columns = 1, cells = {{1}}}}}}, sheet)\nlocal two = sprites.art({{{{size = 16, columns = 1, cells = {{2}}}}}}, sheet)\nlocal g = sprites.group({{slots = 4}})\nlocal function a()\n  local cur = one\n  g:draw(cur, 1, 1)\nend\nfunction frame(t, f)\n  local cur = two\n  g:draw(cur, 2, 2)\n  a()\nend\n"
    );
    let p = ppu_core::sprites::plan(&[("main.luau", &main)]).unwrap().unwrap();
    let arts: Vec<_> = p.calls.iter().map(|c| c.arts.clone()).collect();
    assert_eq!(arts, [Some(vec![1]), Some(vec![2])]);
}

#[test]
fn a_modules_returned_table_of_animations_and_numbers_is_followed_without_refusal() {
    // Leaves the tree can't name but that hold no animation (a number, a string) are not escapes.
    let actor = "local S = require(\"./shared\")\nlocal hop = sprites.animation({S.HERO.down[1], S.HERO.still}, 5)\nlocal ANIMS = {hop = hop, speed = 3}\nreturn {ANIMS = ANIMS, MORE = {hop = hop, name = \"hop\", still = S.HERO.still, hero = S.HERO}}\n";
    let main = "local S = require(\"./shared\")\nlocal A = require(\"./actor\")\nfunction frame(t, f)\n  S.group:draw(A.ANIMS.hop, 1, 1, {at = f})\n  S.group:draw(S.HERO.still, A.ANIMS.speed, 1)\nend\n";
    let p = ppu_core::sprites::plan(&[("shared.luau", HERO), ("actor.luau", actor), ("main.luau", main)]).unwrap().unwrap();
    assert!(p.escapes.is_empty(), "{:?}", p.escapes);
    assert!(p.calls.iter().find(|c| c.line == 4).unwrap().animations.is_some(), "ANIMS.hop is named exactly");
}

/// A draw on a parameter draws into every group its callers give it, through the functions that pass it on; each
/// call's group argument is reported with its group.
#[test]
fn a_draw_on_a_parameter_draws_into_each_group_its_callers_give_it() {
    let row = format!("{SHEET}local dot = sprites.art({{{{size = 16, columns = 1, cells = {{1}}}}}}, sheet)\nlocal function one(g, x)\n  g:draw(dot, x, 1)\nend\nlocal function row(g)\n  for i = 1, 2 do one(g, i * 8) end\nend\nreturn {{row = row}}\n");
    let main = "local Row = require(\"./row\")\nlocal top = sprites.group({slots = 2})\nlocal low = sprites.group({slots = 2})\nfunction frame(t, f)\n  Row.row(top)\n  Row.row(low)\nend\n";
    let p = ppu_core::sprites::plan(&[("row.luau", &row), ("main.luau", main)]).unwrap().unwrap();
    let draws: Vec<_> = p.calls.iter().map(|c| (c.file.as_str(), c.group, c.through.as_deref())).collect();
    assert_eq!(draws, [("row.luau", 0, Some("g")), ("row.luau", 1, Some("g"))]);
    let args: Vec<_> = p.group_args.iter().map(|a| (a.file.as_str(), &main[a.arg.clone()], a.group)).collect();
    assert_eq!(args, [("main.luau", "top", 0), ("main.luau", "low", 1)]);
    // A call that gives it something else is refused on the draw, naming the call.
    let main = main.replace("Row.row(low)", "Row.row(f)");
    let e = ppu_core::sprites::plan(&[("row.luau", &row), ("main.luau", &main)]).unwrap_err();
    let draw = row.lines().position(|l| l.contains("g:draw")).unwrap() as u32 + 1;
    assert_eq!((e.file.as_str(), e.line), ("row.luau", draw), "{e:?}");
    assert!(e.message.contains("the call at main.luau:6 gives it something other than a sprites group"), "{e:?}");
}

#[test]
fn a_modules_own_table_of_functions_is_followed() {
    // `local M = {} function M.show(a) ... end return M`: called from another file and from its own.
    let player = "local S = require(\"./shared\")\nlocal M = {}\nfunction M.show(a, f)\n  S.group:draw(a, 1, 1, {at = f})\nend\nfunction M.walk(f)\n  M.show(S.walk, f)\nend\nreturn M\n";
    let p = ppu_core::sprites::plan(&[("shared.luau", HERO), ("player.luau", player), ("main.luau", "local S = require(\"./shared\")\nlocal P = require(\"./player\")\nfunction frame(t, f)\n  P.show(S.walk, f)\n  P.walk(f)\nend\n")]).unwrap().unwrap();
    assert!(p.escapes.is_empty(), "{:?}", p.escapes);
    assert_eq!(p.calls[0].animations, None, "M.show's art can be the walk");
    // `M.show = function(a, f) ... end`, the same function assigned.
    let player = player.replace("function M.show(a, f)", "M.show = function(a, f)");
    let p = ppu_core::sprites::plan(&[("shared.luau", HERO), ("player.luau", &player), ("main.luau", "local S = require(\"./shared\")\nlocal P = require(\"./player\")\nfunction frame(t, f)\n  P.show(S.walk, f)\nend\n")]).unwrap().unwrap();
    assert!(p.escapes.is_empty(), "{:?}", p.escapes);
    assert_eq!(p.calls[0].animations, None, "an assigned M.show's art can be the walk");
}

#[test]
fn shapes_other_refusals_shield_today_keep_the_animation_or_are_refused_in_the_plan() {
    // Each comes out plain in the plan unless the pass reads it; on the cart another refusal blocks it today.
    let main = |body: &str| format!("local S = require(\"./shared\")\n{body}\n");
    let mut unsound = vec![];
    for (body, line, what) in [
        ("local Hero = {anim = S.walk}\nfunction Hero:render(f)\n  S.group:draw(self.anim, 1, 1, {at = f})\nend\nfunction frame(t, f)\n  Hero:render(f)\nend", 4, "a method's self"),
        ("local Hero = {anim = S.walk}\nfunction Hero.render(self, f)\n  S.group:draw(self.anim, 1, 1, {at = f})\nend\nfunction frame(t, f)\n  Hero:render(f)\nend", 4, "a dot function called with :"),
        ("local Hero = {}\nfunction Hero:draw(a, f)\n  S.group:draw(a, 1, 1, {at = f})\nend\nfunction frame(t, f)\n  Hero:draw(S.walk, f)\nend", 4, "the program's own :draw"),
        ("function frame(t, f)\n  for _, a in ipairs{S.walk} do\n    S.group:draw(a, 1, 1, {at = f})\n  end\nend", 4, "ipairs{...}"),
        ("local function show(a, f)\n  S.group:draw(a, 9, 9)\nend\nlocal function each(show, f)\n  show(S.walk, f)\nend\nfunction frame(t, f)\n  each(function(a, f)\n    S.group:draw(a, 1, 1, {at = f})\n  end, f)\nend", 10, "a parameter named like a function"),
        ("function frame(t, f)\n  S.group:draw(require(\"./shared\").HERO.down, 1, 1, {at = f})\n  S.group:draw(require(\"./actor\").ANIMS.hop, 1, 1, {at = f})\nend", 4, "an inline require's nested field"),
        ("local a = 0\nfunction frame(t, f)\n  a += S.walk\n  S.group:draw(a, 1, 1, {at = f})\nend", 5, "a += walk"),
    ] {
        let actor = "local S = require(\"./shared\")\nlocal hop = sprites.animation({S.HERO.down[1], S.HERO.still}, 5)\nreturn {ANIMS = {hop = hop}}\n";
        if !sound(&[("shared.luau", HERO), ("actor.luau", actor), ("main.luau", &main(body))], "main.luau", line) {
            unsound.push(what);
        }
    }
    assert!(unsound.is_empty(), "the walk reaches a draw marked plain through: {unsound:?}");
}

#[test]
fn the_defences_refuse_nothing_that_holds_no_animation() {
    // A table holding an animation, a forward-declared function, plain art from an inline require.
    let main = "local S = require(\"./shared\")\nlocal A = {walk = S.walk}\nlocal show\nfunction show(a, f)\n  S.group:draw(a, 1, 1, {at = f})\nend\nfunction frame(t, f)\n  show(A.walk, f)\n  S.group:draw(require(\"./shared\").HERO.still, 2, 2)\nend\n";
    let p = ppu_core::sprites::plan(&[("shared.luau", HERO), ("main.luau", main)]).unwrap().unwrap();
    assert!(p.escapes.is_empty(), "{:?}", p.escapes);
    assert!(p.calls.iter().find(|c| c.line == 9).unwrap().animations.is_some(), "plain art through an inline require");
}
