//! `bg[n].columns[c] = dy` (Mode 2 offset-per-tile): the engine derives BG3's table every frame.
//! tests/fixtures/columns_table.json pins the rule frame by frame; the cartridge lowering reads the
//! same file, so the preview and the cart build the same table words.
use ppu_core::{columns_vertical_words, render_frame, LuaEngine};

const TABLE: usize = 0x7800;

fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!("fixtures/columns_table.json")).unwrap()
}

fn round(v: &serde_json::Value) -> i16 {
    ppu_core::scroll_reg(v.as_f64().unwrap() as f32)
}

#[test]
fn the_rule_matches_the_fixture() {
    for case in fixture()["cases"].as_array().unwrap() {
        let layer = case["layer"].as_u64().unwrap() as u8 - 1;
        let width = if case["screen_size"].as_u64().unwrap() & 1 != 0 { 64 } else { 32 };
        let cols = case["columns"].as_object().unwrap();
        for fr in case["frames"].as_array().unwrap() {
            let got = columns_vertical_words(layer, width, round(&fr["scroll_x"]), round(&fr["scroll_y"]), |c| {
                cols.get(&c.to_string()).map_or(0, round)
            });
            let want: Vec<u16> = fr["vertical"].as_array().unwrap().iter().map(|w| w.as_u64().unwrap() as u16).collect();
            assert_eq!(got.to_vec(), want, "{} at {fr}", case["name"]);
        }
    }
}

/// Each fixture frame through a whole program: the words land at bg[3].map_base, row 0 unarmed.
#[test]
fn the_preview_writes_the_fixture_words() {
    for case in fixture()["cases"].as_array().unwrap() {
        let n = case["layer"].as_u64().unwrap();
        let sets: String = case["columns"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(c, dy)| format!("  bg[{n}].columns[{c}] = {dy}\n"))
            .collect();
        let frames = case["frames"].as_array().unwrap();
        let scrolls: String = frames.iter().map(|f| format!("{{{}, {}}}, ", f["scroll_x"], f["scroll_y"])).collect();
        let src = format!(
            "local S = {{{scrolls}}}\nfunction init()\n  mode = 2\n  bg[3].map_base = {TABLE:#x}\n  bg[{n}].screen_size = {}\n{sets}end\n\
             function frame(t, f)\n  bg[{n}].scroll.x = S[f + 1][1]\n  bg[{n}].scroll.y = S[f + 1][2]\nend\n",
            case["screen_size"]
        );
        let mut e = LuaEngine::new();
        e.set_source(&src).unwrap();
        for (f, fr) in frames.iter().enumerate() {
            e.frame(f as f64 / 60.0, f as u32).unwrap();
            let vram = &e.memory().vram;
            let want: Vec<u16> = fr["vertical"].as_array().unwrap().iter().map(|w| w.as_u64().unwrap() as u16).collect();
            assert_eq!(vram[TABLE + 32..TABLE + 64].to_vec(), want, "{} frame {f}", case["name"]);
            assert!(vram[TABLE..TABLE + 32].iter().all(|&w| w == 0), "{} frame {f}: row 0 unarmed", case["name"]);
        }
    }
}

fn run(src: &str, frames: u32) -> Result<LuaEngine, String> {
    let mut e = LuaEngine::new();
    e.set_source(src).map_err(|e| e.message)?;
    for f in 0..frames {
        e.frame(f as f64 / 60.0, f).map_err(|e| e.message)?;
    }
    Ok(e)
}

const SETUP: &str = "mode = 2\nbg[3].map_base = 0x7800\n";

#[test]
fn columns_are_retained_and_nil_or_zero_puts_a_column_back() {
    let e = run(&format!("function init()\n{SETUP}bg[2].columns[1] = 5\nbg[2].columns[2] = 6\nend\n\
        function frame(t, f)\n  if f == 2 then bg[2].columns[1] = nil; bg[2].columns[2] = 0 end\nend\n"), 2).unwrap();
    assert_eq!(e.memory().vram[TABLE + 32..TABLE + 34], [0x4005, 0x4006], "set in init, still there on frame 1");
    let e = run(&format!("function init()\n{SETUP}bg[2].columns[1] = 5\nbg[2].columns[2] = 6\nend\n\
        function frame(t, f)\n  if f == 2 then bg[2].columns[1] = nil; bg[2].columns[2] = 0 end\nend\n"), 3).unwrap();
    assert_eq!(e.memory().vram[TABLE + 32..TABLE + 34], [0, 0]);
}

#[test]
fn no_columns_leave_a_hand_written_table_alone() {
    let e = run("function frame(t, f)\n  mode = 2\n  bg[3].map_base = 0x7800\n  vram[0x7820] = 0x4123\nend\n", 1).unwrap();
    assert_eq!(e.memory().vram[TABLE + 32], 0x4123);
    // ...and outside Mode 2 an unused columns table is no error.
    // A table on a map's words runs (the Studio warns, the cart compiler refuses).
    run("function frame(t, f)\n  mode = 2\n  bg[2].columns[1] = 4\nend\n", 1).unwrap();
    run("function frame(t, f) mode = 1 end\n", 1).unwrap();
}

#[test]
fn a_scene_cold_boot_clears_the_columns() {
    let a = format!("local function init()\n{SETUP}bg[2].columns[3] = 5\nend\n\
        local function frame(t, f) if f == 1 then scenes.go(B) end end\nreturn {{init = init, frame = frame}}\n");
    let b = "local function frame(t, f) left = next(bg[2].columns) end\nreturn {frame = frame}\n";
    let main = "scenes.add(require(\"./a\"))\nB = scenes.add(require(\"./b\"))\nfunction probe() return {left == nil and 1 or 0} end";
    let mut e = LuaEngine::new();
    e.set_sources(&[("main.luau", main), ("a.luau", &a), ("b.luau", b)]).unwrap();
    for f in 0..4 {
        e.frame(f as f64 / 60.0, f).unwrap();
    }
    assert_eq!(e.call_ints("probe", &[]).unwrap(), vec![1]);
}

#[test]
fn what_the_rule_cannot_serve_is_refused_with_its_fix() {
    let refused = |body: &str, says: &str| {
        let err = run(&format!("function frame(t, f)\n{body}\nend\n"), 1).err().unwrap_or_else(|| panic!("{body}: ran"));
        assert!(err.contains(says), "{body}: {err}");
    };
    refused("bg[2].columns[1] = 4", "needs mode = 2");
    refused("mode = 4\nbg[3].map_base = 0x7800\nbg[2].columns[1] = 4", "needs mode = 2");
    refused(&format!("{SETUP}bg[1].columns[1] = 4\nbg[2].columns[1] = 4"), "one layer at a time");
    refused(&format!("{SETUP}bg[3].columns[1] = 4"), "only bg[1] and bg[2]");
    refused(&format!("{SETUP}bg[2].columns[32] = 4"), "32 columns wide");
    refused(&format!("{SETUP}bg[2].tile_size = 16\nbg[2].columns[1] = 4"), "tile_size = 8");
    refused(&format!("{SETUP}bg[3].scroll.x = 8\nbg[2].columns[1] = 4"), "bg[3].scroll at 0, 0");
    refused(&format!("{SETUP}bg[2].columns[1] = \"up\""), "whole number of pixels");
    // A 64-wide map takes columns up to 63.
    run(&format!("function frame(t, f)\n{SETUP}bg[2].screen_size = 1\nbg[2].columns[63] = 4\nend\n"), 1).unwrap();
}

/// A scrolling pipe wall: every map row is pipe except a gap at rows 10..13 (map y 80..111).
/// Columns move the gap; the leftmost screen column never reads the table, and win.w1 0..7 hides it.
const PIPES: &str = r#"
function init()
  mode = 2
  for i = 0, 7 do vram[0x1000 + 16 + i] = 0x00FF end
  for row = 0, 31 do
    if row < 10 or row > 13 then
      bg[2].map[0] = bg[2].map[0] or {}
      for c = 0, 31 do bg[2].map[c] = bg[2].map[c] or {}; bg[2].map[c][row] = {tile = 1} end
    end
  end
  cgram[0] = rgb(80, 160, 255); cgram[1] = rgb(40, 200, 40)
  bg[2].char_base = 0x1000
  bg[3].map_base = 0x7800
  screen.main.bg2 = true
  bg[2].columns[0] = -40
  bg[2].columns[1] = -40
  bg[2].columns[8] = 24
end
function frame(t, f)
  bg[2].scroll.x = f * 20
  if HIDE then win.w1.lo, win.w1.hi = 0, 7; win.bg2.w1 = true; win.bg2.main = true end
end
"#;

fn picture(src: &str, f: u32) -> impl Fn(usize, usize) -> bool {
    let mut e = LuaEngine::new();
    e.set_source(src).unwrap();
    let mut lt = None;
    for k in 0..=f {
        lt = Some(e.frame(k as f64 / 60.0, k).unwrap());
    }
    let rgba = render_frame(&lt.unwrap(), e.memory());
    move |x, y| {
        let p = &rgba[(y * 256 + x) * 4..];
        p[1] > p[2] // green pipe, not blue sky
    }
}

#[test]
fn a_scrolling_pipe_moves_each_column_its_own_way() {
    let pipe = picture(PIPES, 0);
    // Map column 5 has no offset: gap at screen y ~79..110.
    assert!(!pipe(44, 90) && pipe(44, 130), "column 5 unmoved");
    // Column 1 moved by -40: the gap is 40 px lower.
    assert!(pipe(12, 90) && !pipe(12, 130), "column 1 down 40");
    // Column 8 moved by +24: the gap is 24 px higher.
    assert!(!pipe(68, 70) && pipe(68, 100), "column 8 up 24");
    // The leftmost screen column ignores the table: column 0's -40 does not show.
    assert!(!pipe(4, 90) && pipe(4, 130), "leftmost column unmoved");
    // Scrolled 20 px: column 8 is now at screen x 44..51 and keeps its offset.
    let scrolled = picture(PIPES, 1);
    assert!(!scrolled(46, 70) && scrolled(46, 100), "column 8 carries its offset while scrolling");
    // The window fix: win.w1 0..7 on BG2 hides the leftmost column.
    let hidden = picture(&format!("HIDE = true\n{PIPES}"), 0);
    assert!(!hidden(4, 130) && hidden(12, 90), "leftmost column hidden, the rest still drawn");
}
