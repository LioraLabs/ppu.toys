//! PPU-106 :: `dma(name, opts?)` — the init-stage VRAM placement primitive.
//! Top-level (setup) code places sources into VRAM/CGRAM by explicit address;
//! the engine replays the recorded placements each frame (zero -> dma replay
//! -> old binding path -> pokes -> raw vram[]). The payload's committed kind
//! decides the layout, NOT the frame-wide mode — so an m7 payload and a tile
//! bg coexist in one split-screen frame.
mod common;

use ppu_core::{render_frame, LuaEngine};

fn solid(w: usize, h: usize, rgb: [u8; 3]) -> Vec<u8> {
    let mut out = Vec::with_capacity(w * h * 4);
    for _ in 0..w * h {
        out.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
    }
    out
}

/// 8x8 m7 texture with 64 DISTINCT colors (distinct after BGR555 quantize),
/// so at most ONE palette entry can be clobbered by a bg palette sharing
/// CGRAM 1.. — every other texel keeps its own color.
fn track_rgba() -> Vec<u8> {
    let mut out = Vec::with_capacity(8 * 8 * 4);
    for i in 0..64u8 {
        out.extend_from_slice(&[(i % 16) * 16, (i / 16) * 16, 255, 255]);
    }
    out
}

fn green_engine(main: &str) -> LuaEngine {
    let mut e = LuaEngine::new();
    common::add_bg(&mut e, "green", solid(256, 224, [0, 255, 0]), 256, 224, 4);
    e.set_sources(&[("main.lua", main)]).unwrap();
    e
}

#[test]
fn dma_places_a_bg_source_with_explicit_opts() {
    let mut e = green_engine(
        r#"
local g = dma("green", { char = 0x2000, map = 0x1800, pal = 0 })
if g.char ~= 0x2000 then error("bad char") end
if g.map ~= 0x1800 then error("bad map") end
if g.pal ~= 0 then error("bad pal") end
function frame(t, f)
  mode = 1
  bg[1].char_base = g.char
  bg[1].map_base = g.map
  screen.main.bg2 = false; screen.main.bg3 = false
  screen.main.bg4 = false; screen.main.obj = false
end
"#,
    );
    let lt = e.frame(0.0, 0).unwrap();
    let fb = render_frame(&lt, e.memory());
    let p = common::px(&fb, 100, 100);
    assert!(p[1] > 200, "bg pixel should be green, got {p:?}");
    assert!(
        p[0] < 100 && p[2] < 100,
        "bg pixel should be green, got {p:?}"
    );
}

#[test]
fn dma_default_opts_are_char_1000_map_0_pal_0() {
    let mut e = green_engine(
        r#"
local g = dma("green")
if g.char ~= 0x1000 then error("bad default char") end
if g.map ~= 0 then error("bad default map") end
if g.pal ~= 0 then error("bad default pal") end
function frame(t, f)
  mode = 1
  bg[1].char_base = g.char
  bg[1].map_base = g.map
  screen.main.bg2 = false; screen.main.bg3 = false
  screen.main.bg4 = false; screen.main.obj = false
end
"#,
    );
    let lt = e.frame(0.0, 0).unwrap();
    let fb = render_frame(&lt, e.memory());
    let p = common::px(&fb, 100, 100);
    assert!(p[1] > 200, "bg pixel should be green, got {p:?}");
}

/// THE split-screen coexistence case (the point of M11): an m7 payload and a
/// tile bg placed by ONE program, frame-wide mode 1, a `mode = 7` hdma band —
/// both bands render their own content. The old binding path could never do
/// this (it read the frame-wide mode at bind time).
#[test]
fn m7_and_tile_bg_coexist_in_one_split_screen_frame() {
    let mut e = LuaEngine::new();
    common::add_m7(&mut e, "track", track_rgba(), 8, 8);
    common::add_bg(&mut e, "city", solid(256, 224, [0, 255, 0]), 256, 224, 4);
    e.set_sources(&[(
        "main.lua",
        r#"
local m = dma("track")
if m.char ~= 0 or m.map ~= 0 or m.pal ~= 0 then error("bad m7 return") end
local city = dma("city", { char = 0x4000, map = 0x7000 })
function frame(t, f)
  mode = 1
  bg[1].char_base = city.char
  bg[1].map_base = city.map
  screen.main.bg2 = false; screen.main.bg3 = false
  screen.main.bg4 = false; screen.main.obj = false
  hdma(112, 223, function(y)
    mode = 7
  end)
end
"#,
    )])
    .unwrap();
    let lt = e.frame(0.0, 0).unwrap();
    assert_eq!(lt.rows[0].mode, 1, "top band is mode 1");
    assert_eq!(lt.rows[112].mode, 7, "bottom band is mode 7");
    let fb = render_frame(&lt, e.memory());
    // Top band: the tile bg's green.
    let top = common::px(&fb, 100, 50);
    assert!(
        top[1] > 200 && top[0] < 100 && top[2] < 100,
        "top band should show the mode-1 bg, got {top:?}"
    );
    // Bottom band: screen (4,150) maps to track texel (4,6) under the
    // identity matrix — a blue-dominant color no bg palette write clobbers.
    let bot = common::px(&fb, 4, 150);
    assert!(
        bot[2] > 200 && bot[0] < 120 && bot[1] < 120,
        "bottom band should show the m7 texture, got {bot:?}"
    );
}

#[test]
fn m7_source_rejects_explicit_addresses() {
    let mut e = LuaEngine::new();
    common::add_m7(&mut e, "track", track_rgba(), 8, 8);
    let err = e
        .set_sources(&[("main.lua", "dma('track', { char = 0x2000 })")])
        .unwrap_err();
    assert!(err.message.contains("m7"), "got: {}", err.message);
}

/// pal is the CGRAM entry index the palette block lands at — the mode-0
/// banding the old path did implicitly (i*32) is now an explicit opt.
#[test]
fn pal_opt_lands_cgram_at_the_given_index() {
    let mut e = LuaEngine::new();
    common::add_bg(&mut e, "red", solid(256, 224, [255, 0, 0]), 256, 224, 2);
    common::add_bg(&mut e, "blue", solid(256, 224, [0, 0, 255]), 256, 224, 2);
    e.set_sources(&[(
        "main.lua",
        r#"
dma("red",  { char = 0x2000, map = 0x1000, pal = 0 })
dma("blue", { char = 0x3000, map = 0x1400, pal = 32 })
function frame(t, f)
  mode = 0
  bg[1].char_base = 0x2000; bg[1].map_base = 0x1000
  bg[2].char_base = 0x3000; bg[2].map_base = 0x1400
  screen.main.bg3 = false; screen.main.bg4 = false; screen.main.obj = false
end
"#,
    )])
    .unwrap();
    let lt = e.frame(0.0, 0).unwrap();
    let red = e.memory().cgram[1]; // band 0, sub-pal 0, entry 1
    let blue = e.memory().cgram[33]; // band 32 (mode-0 BG2), sub-pal 0, entry 1
    assert!(
        red & 0x1f > 20 && (red >> 10) & 0x1f < 8,
        "cgram[1]={red:#x}"
    );
    assert!((blue >> 10) & 0x1f > 20, "cgram[33]={blue:#x}");
    // bg1 (red, front) renders with its own band.
    let fb = render_frame(&lt, e.memory());
    let p = common::px(&fb, 100, 100);
    assert!(
        p[0] > 200 && p[2] < 100,
        "mode-0 bg1 should be red, got {p:?}"
    );
}

/// A typo'd name is a LOUD init-time error attributed to the calling file —
/// the old path's silent-blank-layer + Mismatch UX dies with the binding.
#[test]
fn unknown_source_name_errors_at_init_in_the_calling_file() {
    let mut e = LuaEngine::new();
    let err = e
        .set_sources(&[("a.lua", "x = 1"), ("b.lua", "dma('nope')")])
        .unwrap_err();
    assert_eq!(err.file.as_deref(), Some("b.lua"));
    assert!(err.message.contains("nope"), "got: {}", err.message);
}

#[test]
fn dma_inside_frame_is_a_runtime_error() {
    let mut e = LuaEngine::new();
    e.set_sources(&[("main.lua", "function frame(t, f) dma('x') end")])
        .unwrap();
    let err = e.frame(0.0, 0).unwrap_err();
    assert!(err.message.contains("setup"), "got: {}", err.message);
    assert_eq!(err.file.as_deref(), Some("main.lua"));
}

#[test]
fn dma_inside_an_hdma_hook_is_a_runtime_error() {
    let mut e = LuaEngine::new();
    e.set_sources(&[(
        "main.lua",
        "function frame(t, f) hdma(0, 10, function(y) dma('x') end) end",
    )])
    .unwrap();
    let err = e.frame(0.0, 0).unwrap_err();
    assert!(err.message.contains("setup"), "got: {}", err.message);
}

/// Placements survive across frames (recorded once at init, replayed after
/// each frame's VRAM/CGRAM zero): consecutive frames are byte-identical.
#[test]
fn replay_is_deterministic_across_frames() {
    let mut e = green_engine(
        r#"
local g = dma("green", { char = 0x2000, map = 0x1800 })
function frame(t, f)
  mode = 1
  bg[1].char_base = g.char
  bg[1].map_base = g.map
end
"#,
    );
    let fb1 = render_frame(&e.frame(0.5, 30).unwrap(), e.memory());
    let fb2 = render_frame(&e.frame(0.5, 30).unwrap(), e.memory());
    assert_eq!(fb1, fb2, "consecutive frames must render identically");
    let p = common::px(&fb1, 100, 100);
    assert!(
        p[1] > 200,
        "placement must survive into later frames, got {p:?}"
    );
}

/// Flush order: zero -> dma replay -> pokes -> raw vram[] final authority. A
/// raw word poke in frame() overrides a dma-placed char word every frame.
#[test]
fn raw_vram_poke_overrides_a_dma_placed_word() {
    // Without the poke, the solid tile's first char word (tile 1, word
    // 0x2010) is dma-placed and nonzero.
    let mut e = green_engine(
        r#"
dma("green", { char = 0x2000, map = 0x1800 })
function frame(t, f) end
"#,
    );
    e.frame(0.0, 0).unwrap();
    let placed = e.memory().vram[0x2010];
    assert_ne!(placed, 0, "dma should have placed char words at 0x2010");
    assert_ne!(placed, 0x1234);
    // With the poke, the raw word wins — on every frame.
    let mut e = green_engine(
        r#"
dma("green", { char = 0x2000, map = 0x1800 })
function frame(t, f)
  vram[0x2010] = 0x1234
end
"#,
    );
    e.frame(0.0, 0).unwrap();
    assert_eq!(e.memory().vram[0x2010], 0x1234);
    e.frame(1.0, 60).unwrap();
    assert_eq!(e.memory().vram[0x2010], 0x1234);
}
