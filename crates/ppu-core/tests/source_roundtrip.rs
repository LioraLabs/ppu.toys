//! Acceptance gate for the format-committed source path: a `convert_source`
//! payload -> `.encode()` -> `add_source` -> `dma()` placement -> `frame`
//! render is well-formed for all source kinds. (The historical `bg[n].source`/
//! `obj.sheet` binding sugar these once exercised is deleted — M11; placement
//! diagnostics live in dma_setup.rs, and the golden-demo suite anchors the
//! source path's exact pixels.)

use ppu_core::{
    convert_source, render_frame_view, rgb15, unpack_rgb15, ConvertOptions, LuaEngine, SourceKind,
    WIDTH,
};

fn fb(engine: &mut LuaEngine, script: &str) -> Vec<u8> {
    engine.set_source(script).unwrap();
    let lt = engine.frame(0.0, 0).unwrap();
    render_frame_view(&lt, engine.memory()).framebuffer
}

/// 16x8: left 8x8 tile solid red (x<12 covers all of cols 0..8), right tile
/// red for x in 8..12 / blue for x in 12..16 -> two distinct tiles.
fn two_tile_rgba() -> Vec<u8> {
    let mut v = Vec::new();
    for _y in 0..8 {
        for x in 0..16 {
            if x < 12 {
                v.extend_from_slice(&[255, 0, 0, 255]);
            } else {
                v.extend_from_slice(&[0, 0, 255, 255]);
            }
        }
    }
    v
}

/// 16x16, four distinct solid 8x8 quadrant colors.
fn quadrant_rgba() -> Vec<u8> {
    let mut rgba = Vec::with_capacity(16 * 16 * 4);
    for y in 0..16u32 {
        for x in 0..16u32 {
            let c: [u8; 4] = match (x < 8, y < 8) {
                (true, true) => [255, 0, 0, 255],     // top-left: red
                (false, true) => [0, 255, 0, 255],    // top-right: green
                (true, false) => [0, 0, 255, 255],    // bottom-left: blue
                (false, false) => [255, 255, 0, 255], // bottom-right: yellow
            };
            rgba.extend_from_slice(&c);
        }
    }
    rgba
}

#[test]
fn bg_payload_renders_from_the_source_store() {
    let rgba = two_tile_rgba();
    let script = "dma('art', { char = 0x1000, map = 0x0000 })\n\
                  function frame(t, f) bg[1].char_base = 0x1000 screen.main.bg1 = true end";

    let mut b = LuaEngine::new();
    let (p, _m) = convert_source(SourceKind::Bg, &ConvertOptions::default(), &rgba, 16, 8).unwrap();
    b.add_source("art", &p.encode()).unwrap();
    let fb_b = fb(&mut b, script);

    assert!(fb_b.chunks(4).any(|px| px[0] > 0 || px[2] > 0));
}

#[test]
fn m7_payload_renders_from_the_source_store() {
    let rgba = quadrant_rgba();
    let script = "dma('floor')\nfunction frame(t, f) mode = 7 screen.main.bg1 = true end";

    let mut b = LuaEngine::new();
    let (p, _m) =
        convert_source(SourceKind::M7, &ConvertOptions::default(), &rgba, 16, 16).unwrap();
    b.add_source("floor", &p.encode()).unwrap();
    let fb_b = fb(&mut b, script);

    assert!(fb_b.chunks(4).any(|px| px[0] > 0 || px[2] > 0));
}

#[test]
fn obj_payload_renders_from_the_source_store() {
    let rgba = two_tile_rgba();
    let (p, meta) =
        convert_source(SourceKind::Obj, &ConvertOptions::default(), &rgba, 16, 8).unwrap();
    let cells = meta.cells.as_ref().unwrap();
    let script = format!(
        "dma('sheet', {{ char = 0x2000 }})\nfunction frame(t, f)\n  obj.char_base = 0x2000\n  screen.main.obj = true\n  obj[0].on = true obj[0].x = 0 obj[0].y = 0 obj[0].tile = {} obj[0].pal = {}\n  obj[1].on = true obj[1].x = 8 obj[1].y = 0 obj[1].tile = {} obj[1].pal = {}\nend",
        cells[0].tile, cells[0].pal, cells[1].tile, cells[1].pal
    );

    let mut b = LuaEngine::new();
    b.add_source("sheet", &p.encode()).unwrap();
    let fb_b = fb(&mut b, &script);

    assert!(fb_b.chunks(4).any(|px| px[0] > 0 || px[2] > 0));
}

#[test]
fn obj_cell16_payload_renders_the_whole_cell_from_one_tile() {
    // cell_size=16 packs a 2x2 tile block per cell; this proves ONE obj[i].tile
    // addresses the whole block via the renderer's name-table stride (+1 right,
    // +16 down).
    let rgba = quadrant_rgba();
    let opts = ConvertOptions {
        cell_size: Some(16),
        ..Default::default()
    };
    let (p, meta) = convert_source(SourceKind::Obj, &opts, &rgba, 16, 16).unwrap();
    let cell = meta.cells.as_ref().unwrap()[0];

    // BG1 rasterizes at its default map_base/char_base = 0, and the
    // cell_size>=16 block importer has no reserved blank tile 0 (unlike the
    // cell_size=8 per-tile path), so OBJ chars placed at 0 would collide with
    // BG1's default read of VRAM address 0 and get opaquely painted over.
    // Place the sheet at a non-overlapping OBJ char base.
    let mut e = LuaEngine::new();
    e.add_source("sheet", &p.encode()).unwrap();
    let script = format!(
        "dma('sheet', {{ char = 0x2000 }})\nfunction frame(t, f)\n  obj.char_base = 0x2000\n  screen.main.obj = true\n  obj.size_sel = 0\n  obj[0].on = true obj[0].large = true obj[0].x = 8 obj[0].y = 8 obj[0].tile = {} obj[0].pal = {}\nend",
        cell.tile, cell.pal
    );
    let render = fb(&mut e, &script);

    // Sprite spans [8,24)x[8,24); sample each quadrant's center.
    let expect_at = |x: usize, y: usize, rgb: [u8; 3]| {
        let expected = unpack_rgb15(rgb15(rgb[0], rgb[1], rgb[2]));
        let o = (y * WIDTH + x) * 4;
        assert_eq!(&render[o..o + 3], &expected[..3], "pixel ({x},{y})");
    };
    expect_at(12, 12, [255, 0, 0]); // top-left quadrant: red
    expect_at(20, 12, [0, 255, 0]); // top-right quadrant: green
    expect_at(12, 20, [0, 0, 255]); // bottom-left quadrant: blue
    expect_at(20, 20, [255, 255, 0]); // bottom-right quadrant: yellow
}

#[test]
fn add_source_rejects_garbage() {
    assert!(LuaEngine::new().add_source("x", &[9, 9, 9]).is_err()); // bad version

    let rgba = two_tile_rgba();
    let (p, _m) = convert_source(SourceKind::Bg, &ConvertOptions::default(), &rgba, 16, 8).unwrap();
    let mut truncated = p.encode();
    truncated.truncate(truncated.len() - 1);
    assert!(LuaEngine::new().add_source("y", &truncated).is_err());
}

// PPU-93: char N is the Nth PNG cell, and `bg[n].map` pokes compose on top of
// a placed sheet against the AUTHOR's map geometry.
#[test]
fn sheet_payload_places_chars_in_sheet_order_for_author_map_pokes() {
    let rgba = quadrant_rgba(); // sheet cells 0..3 = red, green, blue, yellow
    let (p, meta) =
        convert_source(SourceKind::Sheet, &ConvertOptions::default(), &rgba, 16, 16).unwrap();
    let cells = meta.cells.as_ref().unwrap();
    assert_eq!(cells.len(), 4);

    let mut e = LuaEngine::new();
    e.add_source("sheet", &p.encode()).unwrap();
    // Geometry the payload knows nothing about: the author picks map_base and
    // screen_size, then names sheet cells by number.
    let script = format!(
        r#"dma("sheet", {{ char = 0x1000 }})
function frame(t, f)
  mode = 1
  bg[1].map_base = 0x0400
  bg[1].char_base = 0x1000
  bg[1].screen_size = 0
  bg[1].map[0] = {{}} bg[1].map[0][0] = {{tile = 3, pal = {}}}
  bg[1].map[1] = {{}} bg[1].map[1][0] = {{tile = 0, pal = {}}}
  bg[1].map[0][1] = {{tile = 1, pal = {}}}
  screen.main.bg1 = true
end"#,
        cells[3].pal, cells[0].pal, cells[1].pal
    );
    let render = fb(&mut e, &script);

    let expect_at = |x: usize, y: usize, rgb: [u8; 3]| {
        let expected = unpack_rgb15(rgb15(rgb[0], rgb[1], rgb[2]));
        let o = (y * WIDTH + x) * 4;
        assert_eq!(&render[o..o + 3], &expected[..3], "pixel ({x},{y})");
    };
    expect_at(4, 4, [255, 255, 0]); // map cell (0,0) -> sheet cell 3 = yellow
    expect_at(12, 4, [255, 0, 0]); // map cell (1,0) -> sheet cell 0 = red
    expect_at(4, 12, [0, 255, 0]); // map cell (0,1) -> sheet cell 1 = green
}
