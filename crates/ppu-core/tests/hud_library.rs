use ppu_core::LuaEngine;

const HUD: &str = "local HUD = {map = 0x7000, width = 32, height = 4, palette = 7}\n";
const P: u16 = 7 * 1024;

/// Runs `body` in frame(t, f) after `prelude`; returns the engine after frame 0.
fn run(prelude: &str, body: &str) -> Result<LuaEngine, String> {
    let main = format!(
        "local hud = require(\"@ppu/hud\")\n{HUD}{prelude}\nfunction frame(t, f)\n{body}\nend"
    );
    let mut e = LuaEngine::new();
    e.set_sources(&[("main.luau", main.as_str())]).map_err(|e| format!("{e:?}"))?;
    e.frame(0., 0).map_err(|e| format!("{e:?}"))?;
    Ok(e)
}

fn vram(e: &LuaEngine, y: usize, x: usize, n: usize) -> Vec<u16> {
    e.memory().vram[0x7000 + y * 32 + x..0x7000 + y * 32 + x + n].to_vec()
}

fn fails(prelude: &str, body: &str, needle: &str) {
    let err = run(prelude, body).err().expect("should have failed");
    assert!(err.contains(needle), "{needle:?} not in {err}");
}

/// Expected tiles of a field row: digits, or spaces shown as padding.
fn digits(s: &str, zero_pad: bool) -> Vec<u16> {
    s.bytes()
        .map(|b| if b == b' ' { if zero_pad { 16 + P } else { P } } else { (b - b'0') as u16 + 16 + P })
        .collect()
}

#[test]
fn text_field_and_line_write_tiles_with_palette_bits_at_their_cells() {
    let e = run(
        "local score, msg",
        r#"if f == 0 then
  hud.text(HUD, 1, 1, "SCORE")
  score = hud.field(HUD, 7, 1, 6)
  msg = hud.line(HUD, 1, 2, 9)
  hud.text(HUD, 0, 3, "a~", 2)
end"#,
    )
    .unwrap();
    assert_eq!(vram(&e, 1, 1, 5), [51 + P, 35 + P, 47 + P, 50 + P, 37 + P]);
    assert_eq!(vram(&e, 1, 7, 6), [P, P, P, P, P, 16 + P]);
    assert_eq!(vram(&e, 2, 1, 9), [0; 9], "a new line draws nothing");
    assert_eq!(vram(&e, 3, 0, 2), [65 + 2 * 1024, 94 + 2 * 1024]);
}

#[test]
fn field_padding_zero_fills_leading_cells_with_zero_tiles() {
    let e = run(
        "HUD.padding = \"0\"\nlocal s",
        "if f == 0 then s = hud.field(HUD, 3, 0, 4, 5) s:set(42) end",
    )
    .unwrap();
    let t = 5 * 1024;
    assert_eq!(vram(&e, 0, 3, 4), [16 + t, 16 + t, 20 + t, 18 + t]);
}

#[test]
fn field_set_stops_at_the_highest_changed_digit() {
    // Frame 0 sets 123456 and then poisons the five leading cells; frame 1 sets 123457.
    let main = format!(
        "local hud = require(\"@ppu/hud\")\n{HUD}local s\nfunction frame(t, f)
  if f == 0 then
    s = hud.field(HUD, 0, 0, 6)
    s:set(123456)
  elseif f == 1 then
    for i = 0, 4 do vram[0x7000 + i] = 999 end
    s:set(123457)
  elseif f == 2 then
    s:set(654321)
  elseif f == 3 then
    s:set(7)
  elseif f == 4 then
    s:set(1000)
    for i = 0, 5 do vram[0x7000 + i] = 999 end
    s:set(7)
  end
end"
    );
    let mut e = LuaEngine::new();
    e.set_sources(&[("main.luau", main.as_str())]).unwrap();
    e.frame(0., 0).unwrap();
    assert_eq!(vram(&e, 0, 0, 6), digits("123456", false));
    e.frame(0., 1).unwrap();
    assert_eq!(vram(&e, 0, 0, 5), [999; 5], "unchanged leading cells are not rewritten");
    assert_eq!(vram(&e, 0, 5, 1), digits("7", false));
    // Repair the poisoned cells so the next assertion sees a full row.
    e.frame(0., 2).unwrap();
    assert_eq!(vram(&e, 0, 0, 6), digits("654321", false), "every digit changed");
    e.frame(0., 3).unwrap();
    assert_eq!(vram(&e, 0, 0, 6), [P, P, P, P, P, 23 + P], "shrinking blanks the leading cells");
    e.frame(0., 4).unwrap();
    // 7 -> 1000 -> 7: the poison on cells 0..5 survives where 7 and 1000 agree (cells 0, 1).
    assert_eq!(vram(&e, 0, 0, 2), [999; 2]);
    assert_eq!(vram(&e, 0, 2, 4), [P, P, P, 23 + P]);
}

#[test]
fn field_set_rewrites_up_to_the_highest_changed_digit_and_no_further() {
    // 123456 -> 103456: digits up to the changed 2 are rewritten (an unchanged one with its own tile),
    // the leading 1 above it is not touched.
    let main = format!(
        "local hud = require(\"@ppu/hud\")\n{HUD}local s\nfunction frame(t, f)
  if f == 0 then
    s = hud.field(HUD, 0, 0, 6)
    s:set(123456)
  else
    for i = 0, 5 do vram[0x7000 + i] = 999 end
    s:set(103456)
  end
end"
    );
    let mut e = LuaEngine::new();
    e.set_sources(&[("main.luau", main.as_str())]).unwrap();
    e.frame(0., 0).unwrap();
    e.frame(0., 1).unwrap();
    assert_eq!(vram(&e, 0, 0, 1), [999], "the digit above the highest change is not rewritten");
    assert_eq!(vram(&e, 0, 1, 5), digits("03456", false));
}

#[test]
fn unchanged_field_and_line_writes_nothing() {
    let main = format!(
        "local hud = require(\"@ppu/hud\")\n{HUD}local s, l\nfunction frame(t, f)
  if f == 0 then
    s = hud.field(HUD, 0, 0, 3)
    l = hud.line(HUD, 0, 1, 4)
    s:set(5)
    l:set(\"AB\")
  else
    for i = 0, 2 do vram[0x7000 + i] = 999 end
    for i = 0, 3 do vram[0x7020 + i] = 999 end
    s:set(5)
    l:set(\"AB\")
  end
end"
    );
    let mut e = LuaEngine::new();
    e.set_sources(&[("main.luau", main.as_str())]).unwrap();
    e.frame(0., 0).unwrap();
    e.frame(0., 1).unwrap();
    assert_eq!(vram(&e, 0, 0, 3), [999; 3]);
    assert_eq!(vram(&e, 1, 0, 4), [999; 4]);
}

#[test]
fn line_set_shrinks_blanks_only_old_cells_and_accepts_empty() {
    let main = format!(
        "local hud = require(\"@ppu/hud\")\n{HUD}local l\nfunction frame(t, f)
  if f == 0 then
    l = hud.line(HUD, 0, 0, 12)
    vram[0x7000 + 10] = 999
    l:set(\"GAME OVER\")
  elseif f == 1 then
    l:set(\"HI\")
  elseif f == 2 then
    l:set(\"\")
  end
end"
    );
    let mut e = LuaEngine::new();
    e.set_sources(&[("main.luau", main.as_str())]).unwrap();
    e.frame(0., 0).unwrap();
    let go: Vec<u16> = "GAME OVER".bytes().map(|b| b as u16 - 32 + P).collect();
    assert_eq!(vram(&e, 0, 0, 9), go);
    assert_eq!(vram(&e, 0, 10, 1), [999], "cells beyond the text are untouched");
    e.frame(0., 1).unwrap();
    let mut hi = vec![P; 9];
    hi[0] = 40 + P;
    hi[1] = 41 + P;
    assert_eq!(vram(&e, 0, 0, 9), hi, "H, I, then the old cells 3..9 blanked");
    assert_eq!(vram(&e, 0, 9, 2), [0, 999], "cells beyond the old text are untouched");
    e.frame(0., 2).unwrap();
    assert_eq!(vram(&e, 0, 0, 3), [P, P, P], "the empty string blanks what HI covered");
}

#[test]
fn bad_input_is_refused_by_name() {
    fails("", "hud.text(HUD, 30, 0, \"ABC\")", "outside its map");
    fails("", "hud.field(HUD, 0, 4, 3)", "outside its map");
    fails("", "hud.field(HUD, -1, 0, 3)", "outside its map");
    fails("", "hud.line(HUD, 29, 0, 4)", "outside its map");
    fails("", "hud.field(HUD, 0, 0, 0)", "1..9");
    fails("", "hud.field(HUD, 0, 0, 10)", "1..9");
    fails("", "hud.text(HUD, 0.5, 0, \"A\")", "integers");
    fails("", "hud.text(HUD, 0, 0, \"A\", 8)", "palette");
    fails("", "hud.text(HUD, 0, 0, \"A\\n\")", "font");
    fails("", "hud.field(HUD, 0, 0, 3):set(-1)", "fits");
    fails("", "hud.field(HUD, 0, 0, 3):set(1000)", "fits");
    fails("", "hud.field(HUD, 0, 0, 3):set(1.5)", "fits");
    fails("", "hud.line(HUD, 0, 0, 3):set(\"ABCD\")", "wider");
    fails("", "hud.line(HUD, 0, 0, 3):set(\"A\\t\")", "font");
}

#[test]
fn a_failed_set_keeps_the_shown_value() {
    let main = format!(
        "local hud = require(\"@ppu/hud\")\n{HUD}local s, l\nfunction frame(t, f)
  if f == 0 then
    s = hud.field(HUD, 0, 0, 3)
    l = hud.line(HUD, 0, 1, 4)
    s:set(12)
    l:set(\"AB\")
  elseif f == 1 then
    pcall(function() s:set(5000) end)
    pcall(function() l:set(\"Z\\t\") end)
    pcall(function() l:set(\"ZZZZZ\") end)
  else
    s:set(12)
    l:set(\"AB\")
    vram[0x7000] = 999
    vram[0x7020] = 999
    s:set(13)
    l:set(\"AC\")
  end
end"
    );
    let mut e = LuaEngine::new();
    e.set_sources(&[("main.luau", main.as_str())]).unwrap();
    e.frame(0., 0).unwrap();
    e.frame(0., 1).unwrap();
    assert_eq!(vram(&e, 0, 0, 3), [P, 17 + P, 18 + P]);
    // An unknown character is a program error: the cells before it may be drawn, the shown text is kept.
    assert_eq!(vram(&e, 1, 0, 3), [58 + P, 34 + P, 0]);
    // The shown value is still 12 and "AB": an equal set is a no-op, a new one diffs against them.
    e.frame(0., 2).unwrap();
    assert_eq!(vram(&e, 0, 0, 3), [999, 17 + P, 19 + P]);
    assert_eq!(vram(&e, 1, 0, 2), [33 + P, 35 + P], "a line rewrites its glyphs, diffing the length only");
}
