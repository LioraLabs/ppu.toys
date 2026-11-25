//! `bg[n].map:fill(x, y, w, h, entry)`: a rectangle of map cells, bounds checked against the layer's map.
use ppu_core::LuaEngine;

fn run(src: &str) -> Result<LuaEngine, String> {
    let mut e = LuaEngine::new();
    e.set_sources(&[("main.luau", src)]).map_err(|e| format!("{e:?}"))?;
    e.frame(0., 0).map_err(|e| format!("{e:?}"))?;
    Ok(e)
}

#[test]
fn fill_writes_every_cell_of_the_rectangle_and_nothing_else() {
    let e = run(r#"
function init()
  bg[2].map_base = 0x1000
  bg[2].map:fill(1, 2, 3, 30, {tile = 4, pal = 1, flip_x = true})
end
function frame(t, f) end
"#)
    .unwrap();
    let word = 4 | 1 << 10 | 1 << 14;
    let at = |x: usize, y: usize| e.memory().vram[0x1000 + y * 32 + x];
    for y in 0..32 {
        for x in 0..32 {
            let inside = (1..4).contains(&x) && (2..32).contains(&y);
            assert_eq!(at(x, y), if inside { word } else { 0 }, "cell ({x}, {y})");
        }
    }
}

#[test]
fn fill_copies_the_entry_and_keeps_cells_written_around_it() {
    let e = run(r#"
local e = {tile = 7}
bg[1].map[0] = {}
bg[1].map[0][0] = {tile = 9}
bg[1].map:fill(0, 1, 1, 1, e)
e.tile = 8
function frame(t, f) end
"#)
    .unwrap();
    assert_eq!(&e.memory().vram[0..1], &[9]);
    assert_eq!(e.memory().vram[32], 7);
}

#[test]
fn a_wide_layer_takes_64_columns() {
    let e = run("bg[1].screen_size = 1\nbg[1].map:fill(62, 0, 2, 1, {tile = 5})\nfunction frame(t, f) end").unwrap();
    // Columns 32..63 live in the second 32x32 screen, 0x400 words on.
    assert_eq!(e.memory().vram[0x400 + 30], 5);
    assert_eq!(e.memory().vram[0x400 + 31], 5);
}

#[test]
fn fill_refuses_rectangles_and_entries_it_cannot_place() {
    for (src, needle) in [
        ("bg[1].map:fill(30, 0, 3, 1, {tile = 1})", "reaches past the layer's map of 32x32"),
        ("bg[1].map:fill(0, 31, 1, 2, {tile = 1})", "reaches past"),
        ("bg[1].map:fill(-1, 0, 1, 1, {tile = 1})", "x must be a whole number"),
        ("bg[1].map:fill(0, 0, 1.5, 1, {tile = 1})", "w must be a whole number"),
        ("bg[1].map:fill(0, 0, 1, 1, 3)", "the entry must be a table"),
        ("bg[1].map:fill(0, 0, 1, 1, {tile = 1, palette = 2})", "no field palette"),
        ("bg[1].map.fill(0, 0, 1, 1, {tile = 1})", "call fill with a colon"),
    ] {
        let err = run(&format!("{src}\nfunction frame(t, f) end")).err().unwrap_or_else(|| panic!("{src} ran"));
        assert!(err.contains(needle), "{needle:?} not in {err}");
    }
}
