//! The exact two-way map: `import(render(p)) == p` byte for byte over every image source in the
//! in-repo demo fixtures and (when present) the toys tree, plus the refusals and shapes around it.

use std::path::{Path, PathBuf};

use ppu_core::import::obj::ObjCell;
use ppu_core::import::quantize::trim_unused_colors;
use ppu_core::render::{
    decode_png, encode_png, render, render_png, DecodedPng, IndexedImage, IndexedPng,
};
use ppu_core::{
    convert_indexed_source, convert_png_source, convert_source, ConvertOptions, PaletteReport,
    SourceKind, SourceMeta, SourcePayload, SourceReport,
};

/// An RGBA image from a PNG file (any colour type), as the importer reads it today.
fn rgba_of(path: &Path) -> (Vec<u8>, u32, u32) {
    let mut dec = png::Decoder::new(std::fs::File::open(path).unwrap());
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut r = dec.read_info().unwrap();
    let mut buf = vec![0; r.output_buffer_size()];
    let info = r.next_frame(&mut buf).unwrap();
    let px = &buf[..info.buffer_size()];
    let out = match info.color_type {
        png::ColorType::Rgba => px.to_vec(),
        png::ColorType::Rgb => px.chunks(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => px
            .chunks(2)
            .flat_map(|c| [c[0], c[0], c[0], c[1]])
            .collect(),
        png::ColorType::Grayscale => px.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        t => panic!("{path:?}: {t:?}"),
    };
    (out, info.width, info.height)
}

fn dummy_report() -> SourceReport {
    SourceReport::Palette {
        report: PaletteReport {
            rows: 0,
            row_size: 0,
        },
    }
}

/// One image source of a corpus: its name, kind, options, payload and the meta render needs.
struct Entry {
    name: String,
    kind: SourceKind,
    opts: ConvertOptions,
    payload: SourcePayload,
    meta: SourceMeta,
}

fn kind_of(s: &serde_json::Value) -> Option<SourceKind> {
    let k: SourceKind = s["kind"].as_str()?.parse().ok()?;
    matches!(
        k,
        SourceKind::Bg | SourceKind::M7 | SourceKind::Obj | SourceKind::Sheet
    )
    .then_some(k)
}

/// A source that stores its payload (base64) and meta (JSON) instead of a PNG file.
fn stored_entry(file: &str, s: &serde_json::Value, kind: SourceKind) -> Entry {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(
            s["payload"]
                .as_str()
                .expect("a stored source has a payload"),
        )
        .unwrap();
    let m = &s["meta"];
    Entry {
        name: format!("{file} {}", s["name"].as_str().unwrap_or("?")),
        kind,
        opts: serde_json::from_value(s["options"].clone()).unwrap_or_default(),
        payload: SourcePayload::decode(&bytes).unwrap(),
        meta: SourceMeta {
            width: m["width"].as_u64().unwrap() as u32,
            height: m["height"].as_u64().unwrap() as u32,
            report: dummy_report(),
            cells: m
                .get("cells")
                .filter(|c| !c.is_null())
                .map(|c| serde_json::from_value::<Vec<ObjCell>>(c.clone()).unwrap()),
        },
    }
}

/// Every bg/m7/obj/sheet source of web/src/fixtures/demos: the stored payload and meta.
fn fixture_entries() -> Option<Vec<Entry>> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../web/src/fixtures/demos");
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.to_string_lossy().ends_with(".ppu.json"))
        .collect();
    files.sort();
    let mut out = vec![];
    for f in files {
        let j: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&f).unwrap())
            .unwrap_or_else(|e| panic!("{f:?}: {e}"));
        for s in j["sources"].as_array().into_iter().flatten() {
            let Some(kind) = kind_of(s) else { continue };
            out.push(stored_entry(
                &f.file_name().unwrap().to_string_lossy(),
                s,
                kind,
            ));
        }
    }
    Some(out)
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut es: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .collect();
    es.sort();
    for p in es {
        if p.is_dir() {
            if !p.ends_with("node_modules") && !p.ends_with(".ppu") {
                walk(&p, out);
            }
        } else if p.to_string_lossy().ends_with("ppu.json") {
            out.push(p);
        }
    }
}

/// Every bg/m7/obj/sheet PNG source of the monorepo's templates (`../templates` beside this workspace; absent in the
/// engine's public mirror), converted from the PNG's RGBA as importing it always has.
fn template_entries() -> Option<Vec<Entry>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../templates");
    if !root.is_dir() {
        return None;
    }
    let mut files = vec![];
    walk(&root, &mut files);
    let mut out = vec![];
    for f in files {
        let Ok(j) =
            serde_json::from_str::<serde_json::Value>(&std::fs::read_to_string(&f).unwrap())
        else {
            continue;
        };
        for s in j["sources"].as_array().into_iter().flatten() {
            let Some(kind) = kind_of(s) else { continue };
            let Some(file) = s["file"].as_str() else {
                if s["payload"].is_string() {
                    out.push(stored_entry(
                        &f.strip_prefix(&root).unwrap().to_string_lossy(),
                        s,
                        kind,
                    ));
                }
                continue;
            };
            let base = f.parent().unwrap();
            let (img, w, h) = rgba_of(&base.join(file));
            let pri = s["priority_file"]
                .as_str()
                .map(|p| rgba_of(&base.join(p)).0);
            let opts: ConvertOptions =
                serde_json::from_value(s["options"].clone()).unwrap_or_default();
            let name = format!(
                "{} {}",
                f.strip_prefix(&root).unwrap().display(),
                s["name"].as_str().unwrap_or("?")
            );
            let (payload, meta) =
                ppu_core::convert_source_with_priority(kind, &opts, &img, w, h, pri.as_deref())
                    .unwrap_or_else(|e| panic!("{name}: {e}"));
            out.push(Entry {
                name,
                kind,
                opts,
                payload,
                meta,
            });
        }
    }
    Some(out)
}

fn round_trip(entries: &[Entry]) -> Vec<String> {
    let mut failures = vec![];
    for e in entries {
        let result = render_png(&e.payload, &e.meta, None)
            .and_then(|png| convert_png_source(e.kind, &e.opts, &png, None))
            .map(|(p, _)| p.encode() == e.payload.encode());
        match result {
            Ok(true) => {}
            Ok(false) => failures.push(format!("{}: payload differs", e.name)),
            Err(err) => failures.push(format!("{}: {err}", e.name)),
        }
    }
    failures
}

#[test]
fn fixtures_round_trip_byte_for_byte() {
    let Some(entries) = fixture_entries() else {
        eprintln!("skipped: web/src/fixtures/demos is absent (the engine is mirrored without it)");
        return;
    };
    let failures = round_trip(&entries);
    eprintln!("fixtures: {} round-trip entries", entries.len());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn template_sources_round_trip_byte_for_byte() {
    let Some(entries) = template_entries() else {
        eprintln!("skipped: no templates/ beside the engine (the public mirror)");
        return;
    };
    let failures = round_trip(&entries);
    eprintln!("templates: {} round-trip entries", entries.len());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// ---- hand-made art ----

/// An opaque RGBA image from a function of (x, y).
fn rgba(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 3]) -> Vec<u8> {
    let mut out = vec![];
    for y in 0..h {
        for x in 0..w {
            let [r, g, b] = f(x, y);
            out.extend_from_slice(&[r, g, b, 255]);
        }
    }
    out
}

/// An indexed image that is SNES art at `row_size`: entry 0 of each row transparent.
fn art(w: u32, h: u32, row_size: u16, entries: usize, pixels: Vec<u8>) -> IndexedPng {
    let palette: Vec<u16> = (0..entries as u16).map(|i| i * 0x21).collect();
    let alpha = (0..entries)
        .map(|i| if i % row_size as usize == 0 { 0 } else { 255 })
        .collect();
    IndexedPng {
        image: IndexedImage {
            width: w,
            height: h,
            pixels,
            palette,
            row_size,
            extbg: false,
        },
        alpha,
    }
}

/// Render draws a transparent pixel as entry 0 of its unit's row; the art's own convention is 0.
fn unit_neutral(pixels: &[u8], row_size: u8) -> Vec<u8> {
    pixels
        .iter()
        .map(|&p| if p % row_size == 0 { 0 } else { p })
        .collect()
}

fn bg_opts(bit_depth: u8) -> ConvertOptions {
    ConvertOptions {
        bit_depth: Some(bit_depth),
        ..Default::default()
    }
}

#[test]
fn a_rendered_png_has_the_whole_palette_in_rows_and_alpha_0_on_each_entry_0() {
    // 2bpp holds 3 colours a row: tile 0 takes three, tile 1 three others, so two palettes.
    let img = rgba(16, 8, |x, y| match (x < 8, y % 3) {
        (true, 0) => [255, 0, 0],
        (true, 1) => [0, 255, 0],
        (true, _) => [255, 255, 0],
        (_, 0) => [0, 0, 255],
        (_, 1) => [0, 255, 255],
        _ => [255, 0, 255],
    });
    let (p, m) = convert_source(SourceKind::Bg, &bg_opts(2), &img, 16, 8).unwrap();
    let image = render(&p, &m, None).unwrap();
    assert_eq!((image.row_size, image.palette.len()), (4, 8));
    assert_eq!((image.palette[0], image.palette[4]), (0, 0));
    let DecodedPng::Indexed(png) = decode_png(&render_png(&p, &m, None).unwrap()).unwrap() else {
        panic!("a rendered PNG is indexed");
    };
    assert_eq!(png.image.pixels, image.pixels);
    assert_eq!(png.image.palette, image.palette);
    // alpha 0 on entries 0 and 4, 255 elsewhere (trailing 255s are tRNS's default)
    assert_eq!(png.alpha.len(), 8);
    for (i, &a) in png.alpha.iter().enumerate() {
        assert_eq!(a, if i % 4 == 0 { 0 } else { 255 }, "entry {i}");
    }
}

#[test]
fn mode_7_extbg_mirrors_the_palette_and_makes_entry_128_transparent() {
    let img = rgba(8, 8, |x, _| if x < 4 { [255, 0, 0] } else { [0, 0, 255] });
    let pri = rgba(8, 8, |x, _| if x < 4 { [0; 3] } else { [255; 3] });
    let opts = ConvertOptions {
        extbg: Some(true),
        ..Default::default()
    };
    let (p, m) =
        ppu_core::convert_source_with_priority(SourceKind::M7, &opts, &img, 8, 8, Some(&pri))
            .unwrap();
    let image = render(&p, &m, None).unwrap();
    assert!(image.extbg && image.palette.len() == 256);
    assert_eq!(image.palette[1], image.palette[129]);
    let DecodedPng::Indexed(png) = decode_png(&encode_png(&image)).unwrap() else {
        panic!("indexed");
    };
    assert_eq!((png.alpha[0], png.alpha[128]), (0, 0));
    assert!(png.alpha[1..128].iter().all(|&a| a == 255));
    let (back, _) = convert_png_source(SourceKind::M7, &opts, &encode_png(&image), None).unwrap();
    assert_eq!(back.encode(), p.encode());
}

#[test]
fn a_sheet_keeps_its_width_and_cell_order_and_takes_rows_from_meta_cells() {
    // Three 8x8 cells in a row, 24 px wide, cell 1 in palette row 1.
    let mut pixels = vec![0u8; 24 * 8];
    for y in 0..8 {
        for (cell, g) in [(0, 1u8), (1, 17), (2, 2)] {
            pixels[y * 24 + cell * 8 + y.min(7)] = g;
        }
    }
    let png = art(24, 8, 16, 32, pixels);
    let (p, m) = convert_indexed_source(SourceKind::Sheet, &bg_opts(4), &png).unwrap();
    assert_eq!(
        m.cells
            .as_ref()
            .unwrap()
            .iter()
            .map(|c| c.pal)
            .collect::<Vec<_>>(),
        [0, 1, 0]
    );
    let back = render(&p, &m, None).unwrap();
    assert_eq!((back.width, back.height), (24, 8));
    assert_eq!(unit_neutral(&back.pixels, 16), png.image.pixels);
    // the payload alone has no rows for a sheet of two palettes
    let mut bare = m.clone();
    bare.cells = None;
    assert!(render(&p, &bare, None).unwrap_err().contains("meta.cells"));
}

#[test]
fn an_obj_sheet_keeps_its_cell_grid_row_major_at_its_width() {
    // 32x16 px of 16x16 cells: a 2x1 grid, cell 0 palette row 0, cell 1 palette row 2.
    let mut pixels = vec![0u8; 32 * 16];
    for y in 0..16 {
        pixels[y * 32 + y] = 1;
        pixels[y * 32 + 16 + y] = 33;
    }
    let opts = ConvertOptions {
        cell_size: Some(16),
        ..Default::default()
    };
    let png = art(32, 16, 16, 48, pixels);
    let (p, m) = convert_indexed_source(SourceKind::Obj, &opts, &png).unwrap();
    assert_eq!(
        m.cells
            .as_ref()
            .unwrap()
            .iter()
            .map(|c| c.pal)
            .collect::<Vec<_>>(),
        [0, 2]
    );
    assert_eq!(
        unit_neutral(&render(&p, &m, None).unwrap().pixels, 16),
        png.image.pixels
    );
    // a meta that disagrees with the payload is refused
    let mut wide = m.clone();
    wide.width = 48;
    assert!(render(&p, &wide, None).unwrap_err().contains("cells"));
}

// ---- refusals ----

fn refusal(kind: SourceKind, opts: &ConvertOptions, png: &IndexedPng) -> String {
    convert_indexed_source(kind, opts, png).unwrap_err()
}

#[test]
fn a_tile_that_mixes_rows_is_refused_with_its_x_y_and_rows() {
    let mut pixels = vec![0u8; 32 * 64];
    pixels[(7 * 8) * 32 + 3 * 8] = 1; // tile 3,7: row 0 ...
    pixels[(7 * 8) * 32 + 3 * 8 + 1] = 33; // ... and row 2
    let e = refusal(SourceKind::Bg, &bg_opts(4), &art(32, 64, 16, 48, pixels));
    assert_eq!(e, "tile 3,7 mixes rows 0 and 2");
}

#[test]
fn entry_0_of_any_row_is_row_neutral() {
    let mut pixels = vec![0u8; 64];
    pixels[0] = 33; // row 2
    pixels[1] = 32; // row 2's entry 0: transparent, not mixing
    pixels[2] = 16; // row 1's entry 0: same
    let (p, _) =
        convert_indexed_source(SourceKind::Bg, &bg_opts(4), &art(8, 8, 16, 48, pixels)).unwrap();
    let SourcePayload::Bg(bg) = p else { panic!() };
    assert_eq!(bg.palettes.len(), 3);
    assert_eq!(bg.tilemap_words[0] >> 10 & 7, 2);
}

#[test]
fn an_obj_cell_that_mixes_rows_is_refused_as_a_cell() {
    let mut pixels = vec![0u8; 32 * 16];
    pixels[16] = 1; // cell 1,0 row 0
    pixels[8 * 32 + 31] = 17; // cell 1,0 row 1, in its lower right tile
    let opts = ConvertOptions {
        cell_size: Some(16),
        ..Default::default()
    };
    assert_eq!(
        refusal(SourceKind::Obj, &opts, &art(32, 16, 16, 32, pixels)),
        "cell 1,0 mixes rows 0 and 1"
    );
}

#[test]
fn row_and_tile_budgets_are_refused_naming_the_budget() {
    let mut pixels = vec![0u8; 8 * 8];
    pixels[0] = 8 * 16 + 1; // row 8
    let e = refusal(SourceKind::Bg, &bg_opts(4), &art(8, 8, 16, 144, pixels));
    assert!(
        e.contains("row 8") && e.contains("budget is 8 palette rows"),
        "{e}"
    );

    let mut pixels = vec![0u8; 8 * 8];
    pixels[0] = 33; // row 2, but max_palettes 2
    let opts = ConvertOptions {
        max_palettes: Some(2),
        ..Default::default()
    };
    let e = refusal(SourceKind::Sheet, &opts, &art(8, 8, 16, 48, pixels));
    assert!(
        e.contains("row 2") && e.contains("budget is 2 palette rows"),
        "{e}"
    );

    // a sheet keeps no more than 1024 chars
    let e = refusal(
        SourceKind::Sheet,
        &bg_opts(4),
        &art(8200, 8, 16, 1, vec![0; 8200 * 8]),
    );
    assert_eq!(e, "1025 tiles, but the budget keeps 1024");

    // a Mode 7 map keeps 256 unique tiles: 272 tiles (17 x 16) of two pixels each
    let mut pixels = vec![0u8; 136 * 128];
    for t in 0..272usize {
        let (x, y) = (t % 17 * 8, t / 17 * 8);
        pixels[y * 136 + x] = (t % 16 + 1) as u8;
        pixels[y * 136 + x + 1] = (t / 16 + 1) as u8;
    }
    let e = refusal(
        SourceKind::M7,
        &Default::default(),
        &art(136, 128, 256, 18, pixels),
    );
    assert_eq!(e, "272 unique tiles, but a Mode 7 map holds 256");
}

#[test]
fn input_that_is_not_what_it_claims_is_refused() {
    // not SNES art: no tRNS
    let mut png = art(8, 8, 16, 16, vec![1; 64]);
    png.alpha = vec![255; 16];
    assert!(refusal(SourceKind::Bg, &bg_opts(4), &png).starts_with("not SNES art"));
    // a pixel past the PLTE
    let png = art(8, 8, 16, 4, vec![9; 64]);
    assert!(refusal(SourceKind::Bg, &bg_opts(4), &png).contains("PLTE holds 4"));
    // kinds that are not images
    let fine = art(8, 8, 16, 16, vec![1; 64]);
    for k in [SourceKind::Sample, SourceKind::Palette, SourceKind::Song] {
        assert!(refusal(k, &Default::default(), &png).contains("not indexed images"));
        assert!(
            convert_png_source(k, &Default::default(), &encode_png(&fine.image), None).is_err()
        );
    }
    assert!(refusal(SourceKind::Bg, &bg_opts(3), &png).contains("bit_depth"));
    // bytes that are no PNG
    assert!(
        convert_png_source(SourceKind::Bg, &Default::default(), b"GIF89a", None)
            .unwrap_err()
            .contains("not a readable PNG")
    );
    // palette, sample and song payloads do not render
    let (pal, meta) = ppu_core::gpl::convert_palette("GIMP Palette\n1 2 3\n").unwrap();
    assert!(render(&pal, &meta, None).unwrap_err().contains("gpl::write"));
    // a priority mask does not go with SNES art
    let good = encode_png(&art(8, 8, 16, 16, vec![1; 64]).image);
    let e =
        convert_png_source(SourceKind::Bg, &Default::default(), &good, Some(&good)).unwrap_err();
    assert!(e.contains("priority"), "{e}");
}

#[test]
fn zero_size_and_mismatched_buffers_are_refused() {
    let png = art(8, 8, 16, 16, vec![1; 63]);
    assert!(refusal(SourceKind::Bg, &bg_opts(4), &png).contains("mismatch"));
    let (p, mut m) = convert_source(SourceKind::Bg, &bg_opts(4), &[0; 8 * 8 * 4], 8, 8).unwrap();
    m.width = 0;
    assert!(render(&p, &m, None).unwrap_err().contains("no pixels"));
}

// ---- everything else quantizes as today ----

/// A PNG file from RGBA, 8 or 16 bits.
fn rgba_png(w: u32, h: u32, depth: png::BitDepth, data: &[u8]) -> Vec<u8> {
    let mut out = vec![];
    let mut enc = png::Encoder::new(&mut out, w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(depth);
    let mut wr = enc.write_header().unwrap();
    wr.write_image_data(data).unwrap();
    wr.finish().unwrap();
    out
}

#[test]
fn a_full_colour_png_quantizes_exactly_as_convert_source_does() {
    let img = rgba(24, 16, |x, y| {
        [(x * 10) as u8, (y * 15) as u8, ((x ^ y) * 9) as u8]
    });
    let png = rgba_png(24, 16, png::BitDepth::Eight, &img);
    for kind in [
        SourceKind::Bg,
        SourceKind::Sheet,
        SourceKind::Obj,
        SourceKind::M7,
    ] {
        let (a, ma) = convert_png_source(kind, &Default::default(), &png, None).unwrap();
        let (b, mb) = convert_source(kind, &Default::default(), &img, 24, 16).unwrap();
        assert_eq!(a.encode(), b.encode(), "{kind:?}");
        assert_eq!((ma.width, ma.height), (mb.width, mb.height));
    }
}

#[test]
fn an_indexed_png_without_the_snes_signature_quantizes_from_its_colours() {
    // 2 colours, no tRNS: entry 0 is opaque, so this is not SNES art.
    let mut png = art(8, 8, 16, 2, (0..64).map(|i| (i % 2) as u8).collect());
    png.alpha = vec![255; 2];
    png.image.palette = vec![0x001f, 0x7c00];
    let bytes = {
        let mut out = vec![];
        let mut enc = png::Encoder::new(&mut out, 8, 8);
        enc.set_color(png::ColorType::Indexed);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_palette(vec![255, 0, 0, 0, 0, 255]);
        let mut w = enc.write_header().unwrap();
        w.write_image_data(&png.image.pixels).unwrap();
        w.finish().unwrap();
        out
    };
    let DecodedPng::Indexed(dec) = decode_png(&bytes).unwrap() else {
        panic!("indexed");
    };
    assert_eq!(dec.alpha, [255, 255]);
    let (p, _) = convert_png_source(SourceKind::Bg, &Default::default(), &bytes, None).unwrap();
    let rgba: Vec<u8> = png
        .image
        .pixels
        .iter()
        .flat_map(|&i| {
            if i == 0 {
                [255, 0, 0, 255]
            } else {
                [0, 0, 255, 255]
            }
        })
        .collect();
    let (want, _) = convert_source(SourceKind::Bg, &Default::default(), &rgba, 8, 8).unwrap();
    assert_eq!(p.encode(), want.encode());
}

#[test]
fn sub_byte_indexed_pngs_unpack_and_16_bit_ones_are_rgba() {
    let mut out = vec![];
    let mut enc = png::Encoder::new(&mut out, 5, 2);
    enc.set_color(png::ColorType::Indexed);
    enc.set_depth(png::BitDepth::Two);
    enc.set_palette(vec![0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255]);
    enc.set_trns(vec![0]);
    let mut w = enc.write_header().unwrap();
    // two rows of 5 two-bit pixels, each row padded to a byte boundary
    w.write_image_data(&[0b00_01_10_11, 0b01_000000, 0b11_10_01_00, 0b10_000000])
        .unwrap();
    w.finish().unwrap();
    let DecodedPng::Indexed(d) = decode_png(&out).unwrap() else {
        panic!("indexed");
    };
    assert_eq!(d.image.pixels, [0, 1, 2, 3, 1, 3, 2, 1, 0, 2]);
    assert_eq!(d.alpha, [0, 255, 255, 255]);

    let wide: Vec<u8> = (0..8 * 8 * 4 * 2).map(|i| (i % 251) as u8).collect();
    let DecodedPng::Rgba { rgba, width, .. } =
        decode_png(&rgba_png(8, 8, png::BitDepth::Sixteen, &wide)).unwrap()
    else {
        panic!("16-bit colour is RGBA");
    };
    assert_eq!((width, rgba.len()), (8, 8 * 8 * 4));
}

#[test]
fn the_trim_cuts_trailing_colours_no_tile_uses() {
    let mut grid = [0u8; 64];
    grid[0] = 2;
    let pals = vec![vec![1, 2, 3, 4], vec![5, 6], vec![7]];
    let out = trim_unused_colors(pals, &[grid, grid], &[0, 1]);
    // row 0 and 1 use index 2; row 2 is used by nothing
    assert_eq!(out, vec![vec![1, 2], vec![5, 6], vec![]]);
}

#[test]
fn a_256_row_is_entry_0_and_the_real_colours_only_and_writes_to_gpl_as_is() {
    use ppu_core::PaletteSource;
    let p = PaletteSource::from_sub_palettes(256, &[vec![0x1, 0x2, 0x3]]);
    assert_eq!(p.rows, vec![vec![0, 1, 2, 3]]);
    let text = ppu_core::gpl::write(&p, "t");
    assert_eq!(
        text.lines().filter(|l| l.contains('\t')).count(),
        4,
        "{text}"
    );
    // 4 and 16 colour rows stay whole
    assert_eq!(
        PaletteSource::from_sub_palettes(16, &[vec![5]]).rows[0].len(),
        16
    );
}

#[test]
fn an_8bpp_bg_png_has_a_plte_of_1_plus_its_colours_and_round_trips() {
    let img = rgba(8, 8, |x, y| match (x + y) % 3 {
        0 => [255, 0, 0],
        1 => [0, 255, 0],
        _ => [0, 0, 255],
    });
    let (p, m) = convert_source(SourceKind::Bg, &bg_opts(8), &img, 8, 8).unwrap();
    let SourcePayload::Bg(b) = &p else {
        panic!("bg")
    };
    let image = render(&p, &m, None).unwrap();
    assert_eq!(image.row_size, 256);
    assert_eq!(image.palette.len(), 1 + b.palettes[0].len());
    let (back, _) =
        convert_png_source(SourceKind::Bg, &bg_opts(8), &encode_png(&image), None).unwrap();
    assert_eq!(back.encode(), p.encode());
}

#[test]
fn a_priority_mask_of_another_size_is_refused() {
    let opts = ConvertOptions {
        extbg: Some(true),
        ..Default::default()
    };
    let pixels = rgba(16, 8, |x, _| [x as u8 * 16, 0, 0]);
    let img = rgba_png(16, 8, png::BitDepth::Eight, &pixels);
    // Same byte count, transposed: only the decoded sizes tell them apart.
    let mask = rgba_png(8, 16, png::BitDepth::Eight, &rgba(8, 16, |_, _| [255; 3]));
    let err = convert_png_source(SourceKind::M7, &opts, &img, Some(&mask)).unwrap_err();
    assert!(err.contains("8x16") && err.contains("16x8"), "{err}");
    let same = rgba_png(16, 8, png::BitDepth::Eight, &rgba(16, 8, |_, _| [255; 3]));
    convert_png_source(SourceKind::M7, &opts, &img, Some(&same)).unwrap();
}
