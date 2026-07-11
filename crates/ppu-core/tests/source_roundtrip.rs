//! Acceptance gate: a `convert_source` payload -> `.encode()` -> `add_source`
//! render is byte-identical to the direct-import (`upload_asset`) render of
//! the same image + options, for all three source kinds. Two `LuaEngine`s run
//! the SAME script; one uploads the raw asset (classic import path), the
//! other registers the pre-converted payload (source-store path); both are
//! compared framebuffer-for-framebuffer.

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
fn bg_payload_roundtrips_to_identical_render() {
    let rgba = two_tile_rgba();
    let script = r#"function frame(t, f) bg[1].source = "art" end"#;

    let mut a = LuaEngine::new();
    a.upload_asset("art".into(), 16, 8, rgba.clone());
    let fb_a = fb(&mut a, script);

    let mut b = LuaEngine::new();
    let (p, _m) = convert_source(SourceKind::Bg, &ConvertOptions::default(), &rgba, 16, 8).unwrap();
    b.add_source("art", &p.encode()).unwrap();
    let fb_b = fb(&mut b, script);

    assert_eq!(fb_a, fb_b);
    assert!(fb_a.chunks(4).any(|px| px[0] > 0 || px[2] > 0));
}

#[test]
fn m7_payload_roundtrips_to_identical_render() {
    let rgba = quadrant_rgba();
    let script = r#"function frame(t, f) mode = 7 bg[1].source = "floor" end"#;

    let mut a = LuaEngine::new();
    a.upload_asset("floor".into(), 16, 16, rgba.clone());
    let fb_a = fb(&mut a, script);

    let mut b = LuaEngine::new();
    let (p, _m) =
        convert_source(SourceKind::M7, &ConvertOptions::default(), &rgba, 16, 16).unwrap();
    b.add_source("floor", &p.encode()).unwrap();
    let fb_b = fb(&mut b, script);

    assert_eq!(fb_a, fb_b);
    assert!(fb_a.chunks(4).any(|px| px[0] > 0 || px[2] > 0));
}

#[test]
fn obj_payload_roundtrips_to_identical_render() {
    let rgba = two_tile_rgba();
    // Convert once so both engines address the SAME tile numbers (the direct
    // path runs the identical importer under the hood).
    let (p, meta) =
        convert_source(SourceKind::Obj, &ConvertOptions::default(), &rgba, 16, 8).unwrap();
    let cells = meta.cells.as_ref().unwrap();
    let script = format!(
        "function frame(t, f)\n  obj.sheet = \"sheet\"\n  obj[0].on = true obj[0].x = 0 obj[0].y = 0 obj[0].tile = {} obj[0].pal = {}\n  obj[1].on = true obj[1].x = 8 obj[1].y = 0 obj[1].tile = {} obj[1].pal = {}\nend",
        cells[0].tile, cells[0].pal, cells[1].tile, cells[1].pal
    );

    let mut a = LuaEngine::new();
    a.upload_asset("sheet".into(), 16, 8, rgba.clone());
    let fb_a = fb(&mut a, &script);

    let mut b = LuaEngine::new();
    b.add_source("sheet", &p.encode()).unwrap();
    let fb_b = fb(&mut b, &script);

    assert_eq!(fb_a, fb_b);
    assert!(fb_a.chunks(4).any(|px| px[0] > 0 || px[2] > 0));
}

#[test]
fn obj_cell16_payload_renders_the_whole_cell_from_one_tile() {
    // cell_size=16 has no direct-import twin (upload_asset's OBJ path is
    // hardcoded to cell_size 8); this proves ONE obj[i].tile addresses the
    // whole 2x2 block via the renderer's name-table stride (+1 right, +16 down).
    let rgba = quadrant_rgba();
    let opts = ConvertOptions {
        cell_size: Some(16),
        ..Default::default()
    };
    let (p, meta) = convert_source(SourceKind::Obj, &opts, &rgba, 16, 16).unwrap();
    let cell = meta.cells.as_ref().unwrap()[0];

    // BG1 rasterizes at its default map_base/char_base = 0 even when no
    // `source` is bound (the tile-BG import is opt-in; the rasterizer isn't).
    // The cell_size>=16 block importer has no reserved blank tile 0 (unlike
    // the cell_size=8 per-tile path), so an OBJ sheet left at the default
    // char_base=0 collides with BG1's default read of VRAM address 0 and
    // gets opaquely painted over. Bind the sheet to a non-overlapping OBJ
    // char base, exactly like the direct-import OBJ tests do.
    let mut e = LuaEngine::new();
    e.add_source("sheet", &p.encode()).unwrap();
    let script = format!(
        "function frame(t, f)\n  obj.sheet = \"sheet\"\n  obj.char_base = 0x2000\n  obj.size_sel = 0\n  obj[0].on = true obj[0].large = true obj[0].x = 8 obj[0].y = 8 obj[0].tile = {} obj[0].pal = {}\nend",
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

    // Wrong-kind bind: an Obj payload registered under a BG slot doesn't
    // satisfy `apply_imports`'s `SourcePayload::Bg` match arm, so bg[1] stays
    // unbound -> the render must be identical to a fully blank script.
    let (obj_payload, _m) =
        convert_source(SourceKind::Obj, &ConvertOptions::default(), &rgba, 16, 8).unwrap();
    let mut mismatched = LuaEngine::new();
    mismatched.add_source("art", &obj_payload.encode()).unwrap();
    let mis_render = fb(
        &mut mismatched,
        r#"function frame(t, f) bg[1].source = "art" end"#,
    );

    let mut blank = LuaEngine::new();
    let blank_render = fb(&mut blank, "function frame(t, f) end");

    assert_eq!(mis_render, blank_render);
    assert!(!mis_render.chunks(4).any(|px| px[0] > 40 && px[2] > 40));
}
