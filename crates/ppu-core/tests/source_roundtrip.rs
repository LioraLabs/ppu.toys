//! Acceptance gate for the format-committed source path: a `convert_source`
//! payload -> `.encode()` -> `add_source` -> `frame` render is well-formed for
//! all three source kinds, and strict bind validation rejects kind/depth
//! mismatches. (The historical direct-import bridge these once mirrored has
//! been removed; the golden-demo suite now anchors the source path's exact
//! pixels.)

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
    let script = r#"function frame(t, f) bg[1].source = "art" end"#;

    let mut b = LuaEngine::new();
    let (p, _m) = convert_source(SourceKind::Bg, &ConvertOptions::default(), &rgba, 16, 8).unwrap();
    b.add_source("art", &p.encode()).unwrap();
    let fb_b = fb(&mut b, script);

    assert!(fb_b.chunks(4).any(|px| px[0] > 0 || px[2] > 0));
}

#[test]
fn m7_payload_renders_from_the_source_store() {
    let rgba = quadrant_rgba();
    let script = r#"function frame(t, f) mode = 7 bg[1].source = "floor" end"#;

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
        "function frame(t, f)\n  obj.sheet = \"sheet\"\n  obj[0].on = true obj[0].x = 0 obj[0].y = 0 obj[0].tile = {} obj[0].pal = {}\n  obj[1].on = true obj[1].x = 8 obj[1].y = 0 obj[1].tile = {} obj[1].pal = {}\nend",
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
fn source_mismatch_renders_blank_and_reports_diagnostic() {
    use ppu_core::ImportBudget;

    let rgba = two_tile_rgba();

    let mut blank = LuaEngine::new();
    let blank_render = fb(&mut blank, "function frame(t, f) end");

    // Kind mismatch: an OBJ payload bound to bg[1] (a tile-BG slot) must render
    // blank AND surface exactly one obj->bg mismatch diagnostic.
    let (obj_payload, _m) =
        convert_source(SourceKind::Obj, &ConvertOptions::default(), &rgba, 16, 8).unwrap();
    let mut kind = LuaEngine::new();
    kind.add_source("art", &obj_payload.encode()).unwrap();
    let render = fb(
        &mut kind,
        r#"function frame(t, f) bg[1].source = "art" end"#,
    );
    assert_eq!(render, blank_render, "kind mismatch must render blank");
    let reports = kind.import_reports();
    assert_eq!(reports.len(), 1);
    assert!(
        matches!(
            &reports[0],
            ImportBudget::Mismatch { layer: Some(0), found, .. } if found.as_str() == "obj"
        ),
        "expected an obj->bg kind-mismatch diagnostic, got {:?}",
        reports[0]
    );

    // Depth mismatch: a 2bpp BG payload bound to Mode-1 BG1 (4bpp) must render
    // blank AND surface a 2bpp->4bpp depth mismatch (NO down/up conversion).
    let (bg2_payload, _m) = convert_source(
        SourceKind::Bg,
        &ConvertOptions {
            bit_depth: Some(2),
            ..Default::default()
        },
        &rgba,
        16,
        8,
    )
    .unwrap();
    let mut depth = LuaEngine::new();
    depth.add_source("art", &bg2_payload.encode()).unwrap();
    let render2 = fb(
        &mut depth,
        r#"function frame(t, f) mode = 1 bg[1].source = "art" end"#,
    );
    assert_eq!(render2, blank_render, "depth mismatch must render blank");
    let reports2 = depth.import_reports();
    assert_eq!(reports2.len(), 1);
    assert!(
        matches!(
            &reports2[0],
            ImportBudget::Mismatch { layer: Some(0), expected, found, .. }
                if expected.as_str() == "bg 4bpp" && found.as_str() == "bg 2bpp"
        ),
        "expected a 2bpp->4bpp depth-mismatch diagnostic, got {:?}",
        reports2[0]
    );
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
        r#"function frame(t, f)
  mode = 1
  bg[1].source = "sheet"
  bg[1].map_base = 0x0400
  bg[1].char_base = 0x1000
  bg[1].screen_size = 0
  bg[1].map[0] = {{}} bg[1].map[0][0] = {{tile = 3, pal = {}}}
  bg[1].map[1] = {{}} bg[1].map[1][0] = {{tile = 0, pal = {}}}
  bg[1].map[0][1] = {{tile = 1, pal = {}}}
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

// PPU-93: binding a sheet leaves map geometry entirely to the author — unlike
// an assembled bg source, which echoes its own screen_size onto the layer.
#[test]
fn sheet_bind_does_not_echo_map_geometry_but_a_bg_bind_does() {
    let rgba = quadrant_rgba();
    let geometry = r#"function frame(t, f)
  mode = 1
  bg[1].source = "art"
  bg[1].map_base = 0x0400
  bg[1].screen_size = 3
end"#;

    let (sheet, _m) =
        convert_source(SourceKind::Sheet, &ConvertOptions::default(), &rgba, 16, 16).unwrap();
    let mut e = LuaEngine::new();
    e.add_source("art", &sheet.encode()).unwrap();
    e.set_source(geometry).unwrap();
    let row = &e.frame(0.0, 0).unwrap().rows[0];
    assert_eq!(row.bg[0].screen_size, 3, "author screen_size must survive");
    assert_eq!(row.bg[0].map_base, 0x0400, "author map_base must survive");

    // Contrast: the assembled bg source DOES commit its own geometry (a 16x16
    // image imports as a 32x32 screen), which is exactly why sheet is a
    // separate kind rather than a flag.
    let (bg, _m) =
        convert_source(SourceKind::Bg, &ConvertOptions::default(), &rgba, 16, 16).unwrap();
    let mut e2 = LuaEngine::new();
    e2.add_source("art", &bg.encode()).unwrap();
    e2.set_source(geometry).unwrap();
    assert_eq!(e2.frame(0.0, 0).unwrap().rows[0].bg[0].screen_size, 0);
}

// PPU-93: a sheet whose depth doesn't match the slot reports like an assembled
// bg source does, naming the sheet kind honestly.
#[test]
fn sheet_depth_mismatch_reports_a_diagnostic() {
    use ppu_core::ImportBudget;

    let (sheet2, _m) = convert_source(
        SourceKind::Sheet,
        &ConvertOptions {
            bit_depth: Some(2),
            ..Default::default()
        },
        &quadrant_rgba(),
        16,
        16,
    )
    .unwrap();
    let mut e = LuaEngine::new();
    e.add_source("art", &sheet2.encode()).unwrap();

    let mut blank = LuaEngine::new();
    let blank_render = fb(&mut blank, "function frame(t, f) end");
    let render = fb(
        &mut e,
        r#"function frame(t, f) mode = 1 bg[1].source = "art" end"#,
    );
    assert_eq!(render, blank_render, "depth mismatch must render blank");
    let reports = e.import_reports();
    assert_eq!(reports.len(), 1);
    assert!(
        matches!(
            &reports[0],
            ImportBudget::Mismatch { layer: Some(0), expected, found, .. }
                if expected.as_str() == "bg 4bpp" && found.as_str() == "sheet 2bpp"
        ),
        "expected a sheet 2bpp -> bg 4bpp mismatch, got {:?}",
        reports[0]
    );
}

// PPU-93: char_base IS placement, not map geometry — a sheet bound without an
// explicit bg[n].char_base must still render (chars land at bg_bases' 0x1000
// default, so the layer has to learn where they went).
#[test]
fn sheet_bind_without_explicit_char_base_still_renders() {
    let rgba = quadrant_rgba();
    let (p, meta) =
        convert_source(SourceKind::Sheet, &ConvertOptions::default(), &rgba, 16, 16).unwrap();
    let pal = meta.cells.as_ref().unwrap()[3].pal;

    let mut e = LuaEngine::new();
    e.add_source("sheet", &p.encode()).unwrap();
    let script = format!(
        r#"function frame(t, f)
  mode = 1
  bg[1].source = "sheet"
  bg[1].map_base = 0x0400
  bg[1].map[0] = {{}} bg[1].map[0][0] = {{tile = 3, pal = {pal}}}
end"#
    );
    e.set_source(&script).unwrap();
    let lt = e.frame(0.0, 0).unwrap();
    // The layer must report where the chars actually landed.
    assert_eq!(lt.rows[0].bg[0].char_base, 0x1000);
    let render = render_frame_view(&lt, e.memory()).framebuffer;
    let expected = unpack_rgb15(rgb15(255, 255, 0)); // sheet cell 3 = yellow
    assert_eq!(
        &render[(4 * WIDTH + 4) * 4..(4 * WIDTH + 4) * 4 + 3],
        &expected[..3]
    );
}

// PPU-93: a sheet bound where a sheet cannot go names itself honestly — this is
// the path `kind_label` serves (the bg-slot depth mismatch formats its own label).
#[test]
fn sheet_bound_to_mode7_or_obj_reports_as_sheet() {
    use ppu_core::ImportBudget;

    let (p, _m) = convert_source(
        SourceKind::Sheet,
        &ConvertOptions::default(),
        &quadrant_rgba(),
        16,
        16,
    )
    .unwrap();

    let mut m7 = LuaEngine::new();
    m7.add_source("art", &p.encode()).unwrap();
    fb(
        &mut m7,
        r#"function frame(t, f) mode = 7 bg[1].source = "art" end"#,
    );
    assert!(
        matches!(
            &m7.import_reports()[0],
            ImportBudget::Mismatch { expected, found, .. }
                if expected.as_str() == "m7" && found.as_str() == "sheet"
        ),
        "got {:?}",
        m7.import_reports()[0]
    );

    let mut obj = LuaEngine::new();
    obj.add_source("art", &p.encode()).unwrap();
    fb(&mut obj, r#"function frame(t, f) obj.sheet = "art" end"#);
    assert!(
        matches!(
            &obj.import_reports()[0],
            ImportBudget::Mismatch { layer: None, expected, found, .. }
                if expected.as_str() == "obj" && found.as_str() == "sheet"
        ),
        "got {:?}",
        obj.import_reports()[0]
    );
}
