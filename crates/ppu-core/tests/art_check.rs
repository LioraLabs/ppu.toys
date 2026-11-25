//! `check_indexed`: every refusal of the keep path, with positions, and agreement with import.

use ppu_core::render::{
    decode_png, render, render_png, replace_palette, DecodedPng, IndexedImage, IndexedPng,
};
use ppu_core::{
    check_indexed, convert_indexed_source, convert_png_source, convert_source, ConvertOptions,
    PaletteSource, Rule, SourceKind, SourcePayload,
};

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

/// check and import must agree: no findings iff import accepts, else import's error is the first finding.
fn agree(kind: SourceKind, opts: &ConvertOptions, png: &IndexedPng) -> ppu_core::ArtCheck {
    let c = check_indexed(kind, opts, png, None).unwrap();
    match (c.findings.first(), convert_indexed_source(kind, opts, png)) {
        (None, Ok(_)) => {}
        (Some(f), Err(e)) => assert_eq!(f.message, e),
        (f, r) => panic!("check says {f:?}, import says {:?}", r.map(|_| ())),
    }
    c
}

#[test]
fn a_rendered_payload_checks_clean_and_imports() {
    let rgba: Vec<u8> = (0..16 * 16)
        .flat_map(|i| {
            if (i % 16) < 8 {
                [255, 0, 0, 255]
            } else {
                [0, 0, 255, 255]
            }
        })
        .collect();
    let opts = ConvertOptions::default();
    let (p, m) = convert_source(SourceKind::Bg, &opts, &rgba, 16, 16).unwrap();
    let png = render_png(&p, &m, None).unwrap();
    let DecodedPng::Indexed(ip) = decode_png(&png).unwrap() else {
        panic!()
    };
    let c = agree(SourceKind::Bg, &opts, &ip);
    assert!(c.findings.is_empty());
    assert_eq!(
        (c.unit.width, c.budgets.palettes.used, c.budgets.tiles.used),
        (8, 1, 2)
    );
    assert!(convert_png_source(SourceKind::Bg, &opts, &png, None).is_ok());
}

#[test]
fn every_mixed_tile_is_a_finding_with_its_place_and_import_refuses_with_the_first() {
    let mut px = vec![0u8; 16 * 16];
    px[8] = 1; // tile 1,0: rows 0 and 1
    px[9] = 17;
    px[8 * 16] = 2; // tile 0,1: rows 0 and 1
    px[8 * 16 + 1] = 18;
    let c = agree(
        SourceKind::Bg,
        &ConvertOptions::default(),
        &art(16, 16, 16, 32, px),
    );
    let at: Vec<_> = c
        .findings
        .iter()
        .map(|f| (f.rule, f.x, f.y, f.rows.clone()))
        .collect();
    assert_eq!(
        at,
        [
            (Rule::MixedRows, Some(1), Some(0), Some(vec![0, 1])),
            (Rule::MixedRows, Some(0), Some(1), Some(vec![0, 1])),
        ]
    );
}

#[test]
fn entry_0_in_another_row_is_not_mixing_and_a_row_past_the_budget_is_one_finding() {
    let mut px = vec![0u8; 64];
    px[0] = 1;
    px[1] = 16; // entry 0 of row 1: neutral
    let opts = ConvertOptions {
        max_palettes: Some(1),
        ..Default::default()
    };
    assert!(agree(SourceKind::Bg, &opts, &art(8, 8, 16, 32, px))
        .findings
        .is_empty());
    let mut px = vec![0u8; 64];
    px[0] = 17;
    px[1] = 18;
    let c = agree(SourceKind::Bg, &opts, &art(8, 8, 16, 32, px));
    assert_eq!(c.findings.len(), 1);
    assert_eq!(
        (c.findings[0].rule, c.findings[0].rows.clone()),
        (Rule::PaletteBudget, Some(vec![1]))
    );
}

#[test]
fn an_obj_cell_is_the_unit() {
    let mut px = vec![0u8; 16 * 16];
    px[0] = 1;
    px[15 * 16 + 15] = 17; // opposite corners of one 16x16 cell
    let opts = ConvertOptions {
        cell_size: Some(16),
        ..Default::default()
    };
    let c = agree(SourceKind::Obj, &opts, &art(16, 16, 16, 32, px));
    assert_eq!((c.unit.width, c.unit.height), (16, 16));
    assert_eq!((c.findings[0].x, c.findings[0].y), (Some(0), Some(0)));
}

#[test]
fn a_plte_that_is_not_the_palettes_is_found_at_its_first_differing_entry() {
    let png = art(8, 8, 16, 16, vec![1; 64]);
    let rows = vec![png.image.palette.clone()];
    let opts = ConvertOptions::default();
    let mut gpl = PaletteSource { row_size: 16, rows };
    assert!(check_indexed(SourceKind::Bg, &opts, &png, Some(&gpl))
        .unwrap()
        .findings
        .is_empty());
    gpl.rows[0][5] ^= 1;
    let c = check_indexed(SourceKind::Bg, &opts, &png, Some(&gpl)).unwrap();
    assert_eq!(
        (c.findings[0].rule, c.findings[0].index),
        (Rule::Plte, Some(5))
    );
    gpl.rows[0][5] ^= 1;
    gpl.rows.push(vec![0; 16]);
    let c = check_indexed(SourceKind::Bg, &opts, &png, Some(&gpl)).unwrap();
    assert_eq!(
        (c.findings[0].rule, c.findings[0].index),
        (Rule::Plte, Some(16))
    );
}

#[test]
fn a_tile_budget_overflow_names_the_first_tile_past_it() {
    // 4096 tiles, each unique (base 8 digits in pixels 0..3): the 1024th is tile 1023.
    let mut px = vec![0u8; 512 * 512];
    for i in 0..4096usize {
        let (tx, ty) = (i % 64, i / 64);
        for (k, d) in [i % 8, (i / 8) % 8, (i / 64) % 8, i / 512]
            .into_iter()
            .enumerate()
        {
            px[ty * 8 * 512 + tx * 8 + k] = 1 + d as u8;
        }
    }
    let c = agree(
        SourceKind::Bg,
        &ConvertOptions::default(),
        &art(512, 512, 16, 16, px),
    );
    let f = c.findings.last().unwrap();
    assert_eq!((f.rule, f.x, f.y), (Rule::TileBudget, Some(63), Some(15)));
    assert_eq!(c.budgets.tiles.max, 1023);
}

#[test]
fn mode_7_names_the_first_tile_past_256() {
    // 32 x 16 tiles; tile i is the pair (i % 256, i / 256 + 1), so all 512 differ.
    let mut px = vec![0u8; 256 * 128];
    for i in 0..512usize {
        let (tx, ty) = (i % 32, i / 32);
        px[ty * 8 * 256 + tx * 8] = (i % 256) as u8;
        px[ty * 8 * 256 + tx * 8 + 1] = (i / 256 + 1) as u8;
    }
    let c = agree(
        SourceKind::M7,
        &ConvertOptions::default(),
        &art(256, 128, 256, 256, px),
    );
    let f = &c.findings[0];
    assert_eq!((f.rule, f.x, f.y), (Rule::TileBudget, Some(0), Some(8)));
}

#[test]
fn a_png_without_the_signature_is_not_snes_art() {
    let mut png = art(8, 8, 16, 16, vec![1; 64]);
    png.alpha[3] = 0;
    let c = agree(SourceKind::Bg, &ConvertOptions::default(), &png);
    assert_eq!(c.findings[0].rule, Rule::NotSnesArt);
    assert!(check_indexed(SourceKind::Palette, &ConvertOptions::default(), &png, None).is_err());
}

fn chunks(png: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut out = vec![];
    let mut pos = 8;
    while pos < png.len() {
        let n = u32::from_be_bytes(png[pos..pos + 4].try_into().unwrap()) as usize;
        out.push((
            String::from_utf8_lossy(&png[pos + 4..pos + 8]).into(),
            png[pos + 8..pos + 8 + n].to_vec(),
        ));
        pos += 12 + n;
    }
    out
}

fn sample() -> (SourcePayload, ppu_core::SourceMeta) {
    let rgba: Vec<u8> = (0..16 * 16)
        .flat_map(|i| {
            if (i % 16) < 8 {
                [255, 0, 0, 255]
            } else {
                [0, 0, 255, 255]
            }
        })
        .collect();
    convert_source(SourceKind::Bg, &ConvertOptions::default(), &rgba, 16, 16).unwrap()
}

#[test]
fn a_named_palette_is_the_plte_and_import_still_round_trips() {
    let (p, m) = sample();
    let SourcePayload::Bg(b) = &p else { panic!() };
    let mut pal = PaletteSource::from_sub_palettes(16, &b.palettes);
    pal.rows.push(vec![0x7fff; 16]); // an extra row the art does not use
    let img = render(&p, &m, Some(&pal)).unwrap();
    assert_eq!(img.palette, ppu_core::render::plte_of(&pal, false));
    let png = render_png(&p, &m, Some(&pal)).unwrap();
    let (back, _) =
        convert_png_source(SourceKind::Bg, &ConvertOptions::default(), &png, None).unwrap();
    assert_eq!(back.encode(), p.encode());
    let thin = PaletteSource {
        row_size: 16,
        rows: vec![],
    };
    assert!(render(&p, &m, Some(&thin))
        .unwrap_err()
        .contains("palette rows"));
}

#[test]
fn a_named_palette_on_mode_7_extbg_art_is_the_mirrored_plte_and_round_trips() {
    let rgba = |dark: bool| -> Vec<u8> {
        (0..64)
            .flat_map(|i| match (dark, i % 8 < 4) {
                (false, true) => [255, 0, 0, 255],
                (false, false) => [0, 0, 255, 255],
                (true, true) => [0, 0, 0, 255],
                (true, false) => [255, 255, 255, 255],
            })
            .collect()
    };
    let opts = ConvertOptions {
        extbg: Some(true),
        ..Default::default()
    };
    let (p, m) = ppu_core::convert_source_with_priority(
        SourceKind::M7,
        &opts,
        &rgba(false),
        8,
        8,
        Some(&rgba(true)),
    )
    .unwrap();
    let SourcePayload::M7(s) = &p else { panic!() };
    // A 256-row palette that is only 1 + own colours long, as `palette_fits` allows.
    let pal = PaletteSource {
        row_size: 256,
        rows: vec![std::iter::once(0)
            .chain(s.palette.iter().copied())
            .collect()],
    };
    let plte = ppu_core::render::plte_of(&pal, true);
    assert_eq!(plte.len(), 256);
    assert_eq!(plte[130], plte[2]);
    let png = render_png(&p, &m, Some(&pal)).unwrap(); // used to panic: a pixel 130 past a 3-entry PLTE
    let (back, _) = convert_png_source(SourceKind::M7, &opts, &png, None).unwrap();
    assert_eq!(back.encode(), p.encode());
    let DecodedPng::Indexed(ip) = decode_png(&png).unwrap() else {
        panic!()
    };
    assert!(check_indexed(SourceKind::M7, &opts, &ip, Some(&pal))
        .unwrap()
        .findings
        .is_empty());
    assert!(replace_palette(&png, &pal, true).is_ok());
    // A pixel past the PLTE is an Err, never a panic.
    let SourcePayload::M7(_) = &p else { panic!() };
    let no_extbg = ppu_core::render::plte_of(&pal, false);
    assert_eq!(no_extbg.len(), pal.rows[0].len());
    let flat = ConvertOptions::default();
    let (fp, fm) = ppu_core::convert_source(SourceKind::M7, &flat, &rgba(false), 8, 8).unwrap();
    let SourcePayload::M7(fs) = &fp else { panic!() };
    let tiny = PaletteSource {
        row_size: 256,
        rows: vec![vec![0; fs.palette.len()]],
    };
    assert!(render(&fp, &fm, Some(&tiny)).is_err());
}

#[test]
fn a_bg_over_512_px_is_a_map_budget_finding_and_import_refuses_it() {
    let opts = ConvertOptions::default();
    let wide = art(520, 8, 16, 16, vec![0; 520 * 8]);
    let c = agree(SourceKind::Bg, &opts, &wide);
    assert_eq!(
        (c.findings[0].rule, c.findings[0].x, c.findings[0].y),
        (Rule::MapBudget, Some(64), Some(0))
    );
    let tall = art(8, 520, 16, 16, vec![0; 8 * 520]);
    let c = agree(SourceKind::Bg, &opts, &tall);
    assert_eq!(
        (c.findings[0].rule, c.findings[0].x, c.findings[0].y),
        (Rule::MapBudget, Some(0), Some(64))
    );
    let ok = agree(
        SourceKind::Bg,
        &opts,
        &art(512, 512, 16, 16, vec![0; 512 * 512]),
    );
    assert!(ok.findings.is_empty());
}

#[test]
fn replace_palette_changes_only_plte_and_trns() {
    let (p, m) = sample();
    let png = render_png(&p, &m, None).unwrap();
    let new = PaletteSource {
        row_size: 16,
        rows: vec![(0..16).map(|i| i * 0x42).collect(); 2],
    };
    let out = replace_palette(&png, &new, false).unwrap();
    let (a, b) = (chunks(&png), chunks(&out));
    let keep = |c: &[(String, Vec<u8>)]| -> Vec<_> {
        c.iter()
            .filter(|(k, _)| k != "PLTE" && k != "tRNS")
            .cloned()
            .collect()
    };
    assert_eq!(keep(&a), keep(&b)); // IHDR, IDAT, IEND byte for byte
    let (DecodedPng::Indexed(x), DecodedPng::Indexed(y)) =
        (decode_png(&png).unwrap(), decode_png(&out).unwrap())
    else {
        panic!()
    };
    assert_eq!(x.image.pixels, y.image.pixels);
    assert_eq!(y.image.palette, ppu_core::render::plte_of(&new, false));
    assert_eq!((y.alpha[0], y.alpha[16], y.alpha[1]), (0, 0, 255));
    let too_short = PaletteSource {
        row_size: 16,
        rows: vec![vec![0; 2]],
    };
    assert!(replace_palette(&png, &too_short, false)
        .unwrap_err()
        .contains("pixel"));
    let rgba = vec![255u8; 8 * 8 * 4];
    let mut raw = vec![];
    let mut e = png::Encoder::new(&mut raw, 8, 8);
    e.set_color(png::ColorType::Rgba);
    e.set_depth(png::BitDepth::Eight);
    e.write_header().unwrap().write_image_data(&rgba).unwrap();
    assert!(replace_palette(&raw, &new, false)
        .unwrap_err()
        .contains("not an indexed"));
}

#[test]
fn replace_palette_drops_bkgd_and_hist_which_describe_the_old_plte() {
    let (p, m) = sample();
    let png = render_png(&p, &m, None).unwrap();
    let crc = |kind: &[u8], data: &[u8]| {
        let mut c = !0u32;
        for &b in kind.iter().chain(data) {
            c ^= b as u32;
            for _ in 0..8 {
                c = (c >> 1) ^ (0xEDB8_8320 & (c & 1).wrapping_neg());
            }
        }
        (!c).to_be_bytes()
    };
    // Insert a bKGD (one palette index) before the first IDAT.
    let at = 8 + chunks(&png)
        .iter()
        .take_while(|(k, _)| k != "IDAT")
        .map(|(_, d)| 12 + d.len())
        .sum::<usize>();
    let mut with = png[..at].to_vec();
    with.extend_from_slice(&1u32.to_be_bytes());
    with.extend_from_slice(b"bKGD");
    with.push(1);
    with.extend_from_slice(&crc(b"bKGD", &[1]));
    with.extend_from_slice(&png[at..]);
    assert!(chunks(&with).iter().any(|(k, _)| k == "bKGD"));
    let new = PaletteSource {
        row_size: 16,
        rows: vec![vec![0x1234; 16]; 2],
    };
    let out = chunks(&replace_palette(&with, &new, false).unwrap());
    assert!(out.iter().all(|(k, _)| k != "bKGD" && k != "hIST"));
    let idat = |c: &[(String, Vec<u8>)]| -> Vec<_> {
        c.iter().filter(|(k, _)| k == "IDAT").cloned().collect()
    };
    assert_eq!(idat(&out), idat(&chunks(&png)));
}
