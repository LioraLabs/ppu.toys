//! `ppu art check` and `ppu art palette sync` on a temp project, through the built binary.

use ppu_core::render::{encode_png, plte_of, IndexedImage};
use ppu_core::{gpl, PaletteSource};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

fn pal() -> PaletteSource {
    let rows = (0..2)
        .map(|r| {
            (0..16)
                .map(|i| if i == 0 { 0 } else { (r * 16 + i) * 0x21 })
                .collect()
        })
        .collect();
    gpl::parse(&gpl::write(&PaletteSource { row_size: 16, rows }, "pal")).unwrap()
}

fn png(pal: &PaletteSource, w: u32, h: u32, pixels: Vec<u8>) -> Vec<u8> {
    encode_png(&IndexedImage {
        width: w,
        height: h,
        pixels,
        palette: plte_of(pal, false),
        row_size: 16,
        extbg: false,
    })
}

fn project(hero: &[u8], other: &[u8]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    std::fs::write(p.join("pal.gpl"), gpl::write(&pal(), "pal")).unwrap();
    std::fs::write(p.join("hero.png"), hero).unwrap();
    std::fs::create_dir(p.join("art")).unwrap();
    std::fs::write(p.join("art/other.png"), other).unwrap();
    std::fs::write(
        p.join("ppu.json"),
        r#"{"title":"t","files":["main.luau"],"sources":[
 {"name":"pal","kind":"palette","file":"pal.gpl"},
 {"name":"hero","kind":"bg","options":{"palette":"pal"},"file":"hero.png"},
 {"name":"other","kind":"bg","file":"art/other.png"}]}"#,
    )
    .unwrap();
    dir
}

fn ppu(dir: &Path, args: &[&str], stdin: Option<&[u8]>) -> (i32, String, String) {
    let mut c = Command::new(env!("CARGO_BIN_EXE_ppu"))
        .current_dir(dir)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    c.stdin
        .take()
        .unwrap()
        .write_all(stdin.unwrap_or(&[]))
        .unwrap();
    let o = c.wait_with_output().unwrap();
    (
        o.status.code().unwrap(),
        String::from_utf8(o.stdout).unwrap(),
        String::from_utf8(o.stderr).unwrap(),
    )
}

fn mixed() -> Vec<u8> {
    let mut px = vec![0u8; 16 * 16];
    px[8] = 1; // tile 1,0 mixes rows 0 and 1
    px[9] = 17;
    px[8 * 16] = 2; // tile 0,1 too
    px[8 * 16 + 1] = 18;
    px
}

fn findings(out: &str) -> Vec<(String, u64, u64)> {
    let v: serde_json::Value = serde_json::from_str(out).unwrap();
    v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            (
                f["rule"].as_str().unwrap().into(),
                f["x"].as_u64().unwrap(),
                f["y"].as_u64().unwrap(),
            )
        })
        .collect()
}

#[test]
fn the_report_names_the_palette_file_and_the_row_size() {
    let p = pal();
    let dir = project(
        &png(&p, 16, 16, vec![1; 256]),
        &png(&p, 16, 16, vec![1; 256]),
    );
    let gpl = dir.path().canonicalize().unwrap().join("pal.gpl");
    let args = ["art", "check", "hero.png", "--json"];
    let (_, out, _) = ppu(dir.path(), &args, None);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(
        (v["row"].clone(), v["palette_file"].as_str().map(Path::new)),
        (16.into(), Some(gpl.as_path()))
    );
    let stdin = [&args[..], &["--stdin", "--width", "16", "--height", "16"]].concat();
    let (_, out, _) = ppu(dir.path(), &stdin, Some(&[1; 256]));
    let w: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(
        (w["row"].clone(), w["palette_file"].clone()),
        (v["row"].clone(), v["palette_file"].clone())
    );
    // no palette named: a row size, no palette_file
    let (_, out, _) = ppu(
        dir.path(),
        &["art", "check", "art/other.png", "--json"],
        None,
    );
    let o: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert!(o["row"] == 16 && o.get("palette_file").is_none(), "{out}");
    // 2 rows of 16: the transparent entries are the row starts
    assert_eq!(v["transparent"], serde_json::json!([0, 16]));
    assert_eq!(w["transparent"], v["transparent"]);
}

#[test]
fn a_mode_7_extbg_report_lists_entry_128_as_transparent() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    let palette: Vec<u16> = (0..256).collect();
    let bytes = encode_png(&IndexedImage {
        width: 8,
        height: 8,
        pixels: vec![1; 64],
        palette,
        row_size: 256,
        extbg: true,
    });
    std::fs::write(p.join("world.png"), bytes).unwrap();
    std::fs::write(
        p.join("ppu.json"),
        r#"{"title":"t","files":["main.luau"],"sources":[
 {"name":"world","kind":"m7","options":{"extbg":true},"file":"world.png"}]}"#,
    )
    .unwrap();
    let args = [
        "art",
        "check",
        "world.png",
        "--json",
        "--stdin",
        "--width",
        "8",
        "--height",
        "8",
    ];
    let (_, out, _) = ppu(p, &args, Some(&[1; 64]));
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["transparent"], serde_json::json!([0, 128]), "{out}");
    assert_eq!(v["row"], 256);
}

#[test]
fn a_mode_7_extbg_report_gives_the_128_colour_gpl_mirrored_to_256() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    let rows = vec![(0..128).map(|i| i * 0x21 % 0x8000).collect()];
    let pal = gpl::parse(&gpl::write(
        &PaletteSource {
            row_size: 256,
            rows,
        },
        "m7",
    ))
    .unwrap();
    std::fs::write(p.join("m7.gpl"), gpl::write(&pal, "m7")).unwrap();
    let bytes = encode_png(&IndexedImage {
        width: 8,
        height: 8,
        pixels: vec![1; 64],
        palette: plte_of(&pal, true),
        row_size: 256,
        extbg: true,
    });
    std::fs::write(p.join("world.png"), bytes).unwrap();
    std::fs::write(
        p.join("ppu.json"),
        r#"{"title":"t","files":["main.luau"],"sources":[
 {"name":"m7","kind":"palette","file":"m7.gpl","options":{"bit_depth":8}},
 {"name":"world","kind":"m7","options":{"extbg":true,"palette":"m7"},"file":"world.png"}]}"#,
    )
    .unwrap();
    let (_, out, _) = ppu(p, &["art", "check", "world.png", "--json"], None);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["palette_colours"], 128, "{out}");
    assert_eq!(v["plte_own"], 128, "{out}");
    assert_eq!(v["transparent"], serde_json::json!([0, 128]), "{out}");
    let plte = v["plte"].as_array().unwrap();
    assert_eq!(plte.len(), 256, "{out}");
    let [r, g, b, _] = ppu_core::unpack_rgb15(pal.rows[0][5]);
    assert_eq!(plte[5], serde_json::json!([r, g, b]));
    assert_eq!(plte[128 + 5], plte[5], "entry 128 + k is entry k");
}

#[test]
fn a_clean_png_exits_0_and_a_mixed_one_exits_1_with_each_tile() {
    let p = pal();
    let dir = project(&png(&p, 16, 16, vec![1; 256]), &png(&p, 16, 16, mixed()));
    let (code, out, _) = ppu(dir.path(), &["art", "check", "hero.png", "--json"], None);
    assert_eq!(code, 0, "{out}");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(
        (
            v["ok"].clone(),
            v["source"].clone(),
            v["palette"].clone(),
            v["path"].clone()
        ),
        (true.into(), "hero".into(), "pal".into(), "keep".into())
    );
    assert_eq!(v["budgets"]["palettes"]["max"], 8);
    // art/other.png is checked from a subdirectory, found by walking up to ppu.json.
    let (code, out, _) = ppu(
        &dir.path().join("art"),
        &["art", "check", "other.png", "--json"],
        None,
    );
    assert_eq!(code, 1);
    assert_eq!(
        findings(&out),
        [("mixed-rows".into(), 1, 0), ("mixed-rows".into(), 0, 1)]
    );
    let (code, text, _) = ppu(dir.path(), &["art", "check", "art/other.png"], None);
    assert_eq!(code, 1);
    assert!(
        text.contains("at 1,0: tile 1,0 mixes rows 0 and 1"),
        "{text}"
    );
}

#[test]
fn stdin_agrees_with_the_file_and_refuses_a_wrong_length() {
    let p = pal();
    let dir = project(
        &png(&p, 16, 16, vec![1; 256]),
        &png(&p, 16, 16, vec![1; 256]),
    );
    // hero.png on disk is clean; the editor's pixels are the mixed ones.
    let (code, out, _) = ppu(
        dir.path(),
        &[
            "art", "check", "hero.png", "--json", "--stdin", "--width", "16", "--height", "16",
        ],
        Some(&mixed()),
    );
    assert_eq!(code, 1);
    assert_eq!(
        findings(&out),
        [("mixed-rows".into(), 1, 0), ("mixed-rows".into(), 0, 1)]
    );
    let (code, _, err) = ppu(
        dir.path(),
        &[
            "art", "check", "hero.png", "--stdin", "--width", "16", "--height", "16",
        ],
        Some(&[0; 10]),
    );
    assert_eq!(code, 2);
    assert!(err.contains("stdin holds 10 bytes"), "{err}");
    let (code, out, _) = ppu(
        dir.path(),
        &[
            "art", "check", "hero.png", "--json", "--stdin", "--width", "16", "--height", "16",
        ],
        Some(&[0; 10]),
    );
    assert_eq!(code, 2);
    assert!(
        out.contains(r#""ok":false"#) && out.contains("error"),
        "{out}"
    );
}

#[test]
fn a_png_no_source_lists_and_a_missing_manifest_are_refused_by_name() {
    let p = pal();
    let dir = project(&png(&p, 8, 8, vec![1; 64]), &png(&p, 8, 8, vec![1; 64]));
    std::fs::write(dir.path().join("stray.png"), png(&p, 8, 8, vec![1; 64])).unwrap();
    let (code, _, err) = ppu(dir.path(), &["art", "check", "stray.png"], None);
    assert_eq!(code, 2);
    assert!(
        err.contains("stray.png") && err.contains("not the file of any source"),
        "{err}"
    );
    let lone = tempfile::tempdir().unwrap();
    std::fs::write(lone.path().join("a.png"), png(&p, 8, 8, vec![1; 64])).unwrap();
    let (code, _, err) = ppu(lone.path(), &["art", "check", "a.png"], None);
    assert_eq!(code, 2);
    assert!(err.contains("ppu.json"), "{err}");
}

fn idat(png: &[u8]) -> Vec<u8> {
    let (mut pos, mut out) = (8, vec![]);
    while pos < png.len() {
        let n = u32::from_be_bytes(png[pos..pos + 4].try_into().unwrap()) as usize;
        if &png[pos + 4..pos + 8] == b"IDAT" {
            out.extend_from_slice(&png[pos + 8..pos + 8 + n]);
        }
        pos += 12 + n;
    }
    out
}

#[test]
fn palette_sync_rewrites_plte_of_the_pngs_naming_it_and_nothing_else() {
    let old = PaletteSource {
        row_size: 16,
        rows: vec![vec![0x7fff; 16]; 2],
    };
    let (hero, other) = (
        png(&old, 16, 16, vec![1; 256]),
        png(&old, 16, 16, vec![1; 256]),
    );
    let dir = project(&hero, &other);
    let (code, out, err) = ppu(dir.path(), &["art", "palette", "sync", "pal.gpl"], None);
    assert_eq!(code, 0, "{err}");
    assert!(
        out.contains("hero.png") && !out.contains("other.png"),
        "{out}"
    );
    let after = std::fs::read(dir.path().join("hero.png")).unwrap();
    assert_eq!(idat(&after), idat(&hero));
    assert_ne!(after, hero);
    assert_eq!(
        std::fs::read(dir.path().join("art/other.png")).unwrap(),
        other
    );
    let (code, out, _) = ppu(dir.path(), &["art", "check", "hero.png", "--json"], None);
    assert_eq!(code, 0, "{out}");
    assert!(!dir.path().join("hero.png.tmp").exists());
}

#[test]
fn palette_sync_that_cannot_finish_changes_no_file() {
    // The palette has one row; hero's pixels use row 1, so the rewrite is refused.
    let p = pal();
    let hero = png(&p, 16, 16, vec![17; 256]);
    let dir = project(&hero, &hero);
    std::fs::write(
        dir.path().join("pal.gpl"),
        gpl::write(
            &PaletteSource {
                row_size: 16,
                rows: vec![p.rows[0].clone()],
            },
            "pal",
        ),
    )
    .unwrap();
    let (code, _, err) = ppu(dir.path(), &["art", "palette", "sync", "pal.gpl"], None);
    assert_eq!(code, 2);
    assert!(err.contains("hero") && err.contains("pixel"), "{err}");
    assert_eq!(std::fs::read(dir.path().join("hero.png")).unwrap(), hero);
}

#[test]
fn pack_reads_a_gpl_palette_source_and_keeps_snes_art_indices() {
    let p = pal();
    let dir = project(&png(&p, 8, 8, vec![1; 64]), &png(&p, 8, 8, vec![1; 64]));
    std::fs::write(dir.path().join("main.luau"), "function frame() end").unwrap();
    let packed: serde_json::Value =
        serde_json::from_str(&ppu_cli::pack(dir.path()).unwrap()).unwrap();
    assert_eq!(packed["sources"].as_array().unwrap().len(), 3);
}

/// `png` with its tRNS dropped (an editor's save): the PLTE and pixels stay.
fn without_trns(png: &[u8]) -> Vec<u8> {
    let (mut pos, mut out) = (8, png[..8].to_vec());
    while pos < png.len() {
        let n = u32::from_be_bytes(png[pos..pos + 4].try_into().unwrap()) as usize;
        if &png[pos + 4..pos + 8] != b"tRNS" {
            out.extend_from_slice(&png[pos..pos + 12 + n]);
        }
        pos += 12 + n;
    }
    out
}

#[test]
fn a_palette_named_png_that_lost_its_trns_is_not_snes_art_and_still_gets_the_other_rules() {
    let p = pal();
    let mut bad = pal();
    bad.rows[0][3] = 0x1234;
    let dir = project(
        &without_trns(&png(&p, 16, 16, mixed())),
        &without_trns(&png(&bad, 16, 16, vec![1; 256])),
    );
    let (code, out, _) = ppu(dir.path(), &["art", "check", "hero.png", "--json"], None);
    assert_eq!(code, 1, "{out}");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["path"], "keep");
    assert_eq!(v["findings"][0]["rule"], "not-snes-art");
    // The no-tRNS PNG naming a palette is still checked for mixed tiles and PLTE != gpl.
    let (code, text, _) = ppu(dir.path(), &["art", "check", "hero.png"], None);
    assert_eq!(code, 1);
    assert!(
        text.contains("not-snes-art") && text.contains("mixed-rows at 1,0"),
        "{text}"
    );
    // art/other.png names no palette, so it keeps the quantize path; give it one to see the PLTE rule.
    let json = std::fs::read_to_string(dir.path().join("ppu.json")).unwrap();
    std::fs::write(
        dir.path().join("ppu.json"),
        json.replace(
            r#""kind":"bg","file":"art/other.png""#,
            r#""kind":"bg","options":{"palette":"pal"},"file":"art/other.png""#,
        ),
    )
    .unwrap();
    let (code, out, _) = ppu(
        dir.path(),
        &["art", "check", "art/other.png", "--json"],
        None,
    );
    assert_eq!(code, 1, "{out}");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let rules: Vec<_> = v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["rule"].as_str().unwrap())
        .collect();
    assert_eq!(rules, ["not-snes-art", "plte"]);
    assert_eq!(v["findings"][1]["index"], 3);
}

#[test]
fn an_rgba_png_naming_a_palette_is_not_snes_art_and_one_naming_none_quantizes() {
    let rgba = |dir: &Path, name: &str| {
        let mut e = png::Encoder::new(std::fs::File::create(dir.join(name)).unwrap(), 8, 8);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        e.write_header()
            .unwrap()
            .write_image_data(&[200u8; 8 * 8 * 4])
            .unwrap();
    };
    let p = pal();
    let dir = project(&png(&p, 8, 8, vec![1; 64]), &png(&p, 8, 8, vec![1; 64]));
    rgba(dir.path(), "hero.png");
    rgba(&dir.path().join("art"), "other.png");
    let (code, out, _) = ppu(dir.path(), &["art", "check", "hero.png", "--json"], None);
    assert_eq!(code, 1, "{out}");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(
        (v["path"].clone(), v["findings"][0]["rule"].clone()),
        ("quantize".into(), "not-snes-art".into())
    );
    let (code, out, _) = ppu(
        dir.path(),
        &["art", "check", "art/other.png", "--json"],
        None,
    );
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&out).unwrap()["path"],
        "quantize"
    );
}

#[test]
fn palette_sync_refuses_a_malformed_options_palette_like_check() {
    let p = pal();
    let dir = project(&png(&p, 8, 8, vec![1; 64]), &png(&p, 8, 8, vec![1; 64]));
    let json = std::fs::read_to_string(dir.path().join("ppu.json")).unwrap();
    std::fs::write(
        dir.path().join("ppu.json"),
        json.replace(
            r#""kind":"bg","file":"art/other.png""#,
            r#""kind":"bg","options":{"palette":3},"file":"art/other.png""#,
        ),
    )
    .unwrap();
    let (code, _, err) = ppu(dir.path(), &["art", "palette", "sync", "pal.gpl"], None);
    assert_eq!(code, 2);
    assert!(err.contains("options.palette"), "{err}");
}

#[test]
fn an_obj_block_sheet_reports_the_cap_its_cells_fit() {
    let p = pal();
    let dir = project(&png(&p, 8, 8, vec![1; 64]), &png(&p, 8, 8, vec![1; 64]));
    let json = std::fs::read_to_string(dir.path().join("ppu.json")).unwrap();
    std::fs::write(
        dir.path().join("ppu.json"),
        json.replace(
            r#""kind":"bg","options":{"palette":"pal"}"#,
            r#""kind":"obj","options":{"palette":"pal","cell_size":24}"#,
        ),
    )
    .unwrap();
    let (code, out, _) = ppu(
        dir.path(),
        &[
            "art", "check", "hero.png", "--json", "--stdin", "--width", "24", "--height", "24",
        ],
        Some(&[1; 24 * 24]),
    );
    assert_eq!(code, 0, "{out}");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["budgets"]["tiles"]["max"], 450);
}

#[test]
fn a_second_palette_sync_writes_nothing_and_lists_nothing() {
    let old = PaletteSource {
        row_size: 16,
        rows: vec![vec![0x7fff; 16]; 2],
    };
    let hero = png(&old, 16, 16, vec![1; 256]);
    let dir = project(&hero, &hero);
    let sync = || ppu(dir.path(), &["art", "palette", "sync", "pal.gpl"], None);
    let (code, out, err) = sync();
    assert_eq!((code, out.contains("hero.png")), (0, true), "{err}");
    let first = std::fs::metadata(dir.path().join("hero.png"))
        .unwrap()
        .modified()
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(50));
    let (code, out, err) = sync();
    assert_eq!(code, 0, "{err}");
    assert!(out.trim().is_empty(), "{out}");
    let second = std::fs::metadata(dir.path().join("hero.png"))
        .unwrap()
        .modified()
        .unwrap();
    assert_eq!(first, second);
}

#[test]
fn a_png_listed_by_two_sources_is_refused() {
    let p = pal();
    let hero = png(&p, 8, 8, vec![1; 64]);
    let dir = project(&hero, &hero);
    let json = std::fs::read_to_string(dir.path().join("ppu.json")).unwrap();
    std::fs::write(
        dir.path().join("ppu.json"),
        json.replace(
            r#"{"name":"other","kind":"bg","file":"art/other.png"}"#,
            r#"{"name":"again","kind":"obj","file":"hero.png"}"#,
        ),
    )
    .unwrap();
    let (code, _, err) = ppu(dir.path(), &["art", "check", "hero.png"], None);
    assert_eq!(code, 2);
    assert!(
        err.contains("listed by sources \"hero\" and \"again\""),
        "{err}"
    );
}

/// `ppu` with raw stdout, for the commands whose output is index bytes.
fn ppu_bytes(dir: &Path, args: &[&str], stdin: &[u8]) -> (i32, Vec<u8>, String) {
    let mut c = Command::new(env!("CARGO_BIN_EXE_ppu"))
        .current_dir(dir)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    c.stdin.take().unwrap().write_all(stdin).unwrap();
    let o = c.wait_with_output().unwrap();
    (
        o.status.code().unwrap(),
        o.stdout,
        String::from_utf8(o.stderr).unwrap(),
    )
}

fn plte(path: &Path) -> Vec<u16> {
    match ppu_core::render::decode_png(&std::fs::read(path).unwrap()).unwrap() {
        ppu_core::render::DecodedPng::Indexed(ip) => ip.image.palette,
        _ => panic!("not indexed"),
    }
}

#[test]
fn palette_write_snaps_the_colours_keeps_the_names_and_syncs_the_pngs() {
    let p = pal();
    let hero = png(&p, 16, 16, vec![1; 256]);
    let dir = project(&hero, &hero);
    let named: String = std::fs::read_to_string(dir.path().join("pal.gpl"))
        .unwrap()
        .replace("\t1\n", "\tskin\n");
    std::fs::write(dir.path().join("pal.gpl"), &named).unwrap();
    let mut stdin = String::new();
    for i in 0..32 {
        stdin += &format!("{} 0 0\n", if i == 1 { 255 } else { 0 });
    }
    let (code, out, err) = ppu(
        dir.path(),
        &["art", "palette", "write", "pal.gpl"],
        Some(stdin.as_bytes()),
    );
    assert_eq!(code, 0, "{err}");
    assert!(
        out.contains("hero.png") && !out.contains("other.png"),
        "{out}"
    );
    let text = std::fs::read_to_string(dir.path().join("pal.gpl")).unwrap();
    assert!(
        text.starts_with("GIMP Palette\nName: pal\nColumns: 16\n#\n"),
        "{text}"
    );
    assert!(text.contains("255   0   0\tskin\n"), "{text}");
    assert_eq!(plte(&dir.path().join("hero.png"))[1], 0x001f);
    // the same colours again change nothing
    let (code, out, _) = ppu(
        dir.path(),
        &["art", "palette", "write", "pal.gpl"],
        Some(stdin.as_bytes()),
    );
    assert_eq!((code, out.as_str()), (0, ""));
}

#[test]
fn palette_write_refuses_a_wrong_count_or_a_bad_line_and_changes_nothing() {
    let hero = png(&pal(), 16, 16, vec![1; 256]);
    let dir = project(&hero, &hero);
    let before = std::fs::read(dir.path().join("pal.gpl")).unwrap();
    for (stdin, want) in [
        ("1 2 3\n", "32 colour lines"),
        ("1 2\n", "stdin line 1"),
        ("1 2 300\n", "stdin line 1"),
    ] {
        let (code, _, err) = ppu(
            dir.path(),
            &["art", "palette", "write", "pal.gpl"],
            Some(stdin.as_bytes()),
        );
        assert_eq!(code, 2, "{stdin}");
        assert!(err.contains(want), "{err}");
    }
    assert_eq!(std::fs::read(dir.path().join("pal.gpl")).unwrap(), before);
    assert_eq!(std::fs::read(dir.path().join("hero.png")).unwrap(), hero);
    // a .gpl whose Columns disagree with the art's rows names both sizes as plain numbers
    let narrow = PaletteSource {
        row_size: 4,
        rows: pal().rows.concat().chunks(4).map(<[u16]>::to_vec).collect(),
    };
    let narrow = gpl::write(&narrow, "pal");
    std::fs::write(dir.path().join("pal.gpl"), &narrow).unwrap();
    let stdin = "0 0 0\n".repeat(32);
    let (code, _, err) = ppu(
        dir.path(),
        &["art", "palette", "write", "pal.gpl"],
        Some(stdin.as_bytes()),
    );
    assert_eq!(code, 2, "{err}");
    assert!(
        err.contains("source \"hero\": its rows are 16 colours, but palette \"pal\" has 4"),
        "{err}"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("pal.gpl")).unwrap(),
        narrow
    );
    assert_eq!(std::fs::read(dir.path().join("hero.png")).unwrap(), hero);
}

#[test]
fn rows_set_moves_the_touched_tiles_and_fix_pulls_strays_into_the_tiles_row() {
    let hero = png(&pal(), 16, 16, vec![0; 256]);
    let dir = project(&hero, &hero);
    let mut px = vec![0u8; 256];
    px[0] = 1;
    px[8] = 2; // tile 1,0
    let (code, out, err) = ppu_bytes(
        dir.path(),
        &[
            "art", "rows", "hero.png", "--width", "16", "--height", "16", "--set", "1", "--rect",
            "8,0,1,1",
        ],
        &px,
    );
    assert_eq!(code, 0, "{err}");
    assert_eq!((out[0], out[8]), (1, 18));
    let (code, _, err) = ppu_bytes(
        dir.path(),
        &[
            "art", "rows", "hero.png", "--width", "16", "--height", "16", "--set", "2", "--rect",
            "0,0,1,1",
        ],
        &px,
    );
    assert_eq!(code, 2);
    assert!(err.contains("past"), "{err}");
    // pal(): row 1 colour i is (16+i)*0x21, so no row 0 entry has the same colour: the stray stays
    let mut stray = vec![0u8; 256];
    stray[..3].copy_from_slice(&[1, 2, 17]);
    let (code, out, err) = ppu_bytes(
        dir.path(),
        &[
            "art", "rows", "hero.png", "--width", "16", "--height", "16", "--fix",
        ],
        &stray,
    );
    assert_eq!(code, 0, "{err}");
    assert_eq!(out[..3], [1, 2, 17]);
    assert!(
        err.contains("tile 0,0: 1 pixels have no colour in row 0"),
        "{err}"
    );
    // give row 1's entry 1 the colour of row 0's entry 1 and the stray moves
    let mut shared = pal();
    shared.rows[1][1] = shared.rows[0][1];
    std::fs::write(dir.path().join("pal.gpl"), gpl::write(&shared, "pal")).unwrap();
    let (code, out, err) = ppu_bytes(
        dir.path(),
        &[
            "art", "rows", "hero.png", "--width", "16", "--height", "16", "--fix",
        ],
        &stray,
    );
    assert_eq!(code, 0, "{err}");
    assert_eq!(out[..3], [1, 2, 1]);
}

#[test]
fn palette_set_remaps_by_exact_colour_and_points_the_source_at_the_new_gpl() {
    let hero = png(&pal(), 16, 16, vec![0; 256]);
    let dir = project(&hero, &hero);
    // new palette: pal()'s colours, rows swapped
    let p = pal();
    let swapped = PaletteSource {
        row_size: 16,
        rows: vec![p.rows[1].clone(), p.rows[0].clone()],
    };
    std::fs::write(dir.path().join("swap.gpl"), gpl::write(&swapped, "swap")).unwrap();
    let mut px = vec![0u8; 256];
    px[0] = 1;
    px[1] = 3;
    px[8] = 17;
    let args = [
        "art", "palette", "set", "hero.png", "swap.gpl", "--width", "16", "--height", "16",
    ];
    let (code, out, err) = ppu_bytes(dir.path(), &args, &px);
    assert_eq!(code, 0, "{err}");
    assert_eq!((out[0], out[1], out[8]), (17, 19, 1));
    // the PNG on disk is the remapped art under the new palette, and the check is clean
    let on_disk =
        ppu_core::render::decode_png(&std::fs::read(dir.path().join("hero.png")).unwrap()).unwrap();
    let ppu_core::render::DecodedPng::Indexed(ip) = on_disk else {
        panic!("hero.png is no longer indexed")
    };
    assert_eq!(ip.image.pixels, out);
    assert_eq!(ip.image.palette, plte_of(&swapped, false));
    let (code, _, err) = ppu(dir.path(), &["art", "check", "hero.png"], None);
    assert_eq!(code, 0, "{err}");
    let manifest = std::fs::read_to_string(dir.path().join("ppu.json")).unwrap();
    assert!(
        manifest.contains(r#""name": "swap""#) && manifest.contains(r#""file": "swap.gpl""#),
        "{manifest}"
    );
    let v: serde_json::Value = serde_json::from_str(&manifest).unwrap();
    assert_eq!(v["sources"][1]["options"]["palette"], "swap");
    assert_eq!(v["sources"][2]["name"], "other");
    assert!(!dir.path().join(".ppu").read_dir().unwrap().any(|e| e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .ends_with(".tmp")));
}

#[test]
fn palette_set_refuses_a_non_object_options_by_naming_the_source() {
    let hero = png(&pal(), 16, 16, vec![0; 256]);
    let dir = project(&hero, &hero);
    let json = std::fs::read_to_string(dir.path().join("ppu.json")).unwrap();
    let mut v: serde_json::Value = serde_json::from_str(&json).unwrap();
    let i = v["sources"]
        .as_array()
        .unwrap()
        .iter()
        .position(|s| s["file"].as_str().is_some_and(|f| f.ends_with("hero.png")))
        .unwrap();
    v["sources"][i]["options"] = serde_json::json!([]);
    std::fs::write(dir.path().join("ppu.json"), v.to_string()).unwrap();
    let before = std::fs::read(dir.path().join("ppu.json")).unwrap();
    std::fs::write(dir.path().join("new.gpl"), gpl::write(&pal(), "new")).unwrap();
    let args = [
        "art", "palette", "set", "hero.png", "new.gpl", "--width", "16", "--height", "16",
    ];
    let (code, out, err) = ppu_bytes(dir.path(), &args, &[0u8; 256]);
    assert_eq!((code, out.len()), (2, 0), "{err}");
    assert!(err.contains("hero") && err.contains("options"), "{err}");
    assert_eq!(std::fs::read(dir.path().join("ppu.json")).unwrap(), before);
}

#[test]
fn palette_set_with_a_colour_the_new_palette_lacks_reports_it_and_changes_nothing() {
    let hero = png(&pal(), 16, 16, vec![0; 256]);
    let dir = project(&hero, &hero);
    let mut lacking = pal();
    lacking.rows[0][1] = 0x7fff;
    std::fs::write(dir.path().join("new.gpl"), gpl::write(&lacking, "new")).unwrap();
    let before = std::fs::read(dir.path().join("ppu.json")).unwrap();
    let mut px = vec![0u8; 256];
    px[0] = 1; // pal() colour 0x21: new.gpl has it nowhere in row 0, and row 1 is (16+i)*0x21
    let args = [
        "art", "palette", "set", "hero.png", "new.gpl", "--width", "16", "--height", "16",
    ];
    let (code, out, err) = ppu_bytes(dir.path(), &args, &px);
    assert_eq!((code, out.len()), (1, 0), "{err}");
    assert!(err.contains("#08") && err.contains("tile 0,0"), "{err}");
    assert_eq!(std::fs::read(dir.path().join("ppu.json")).unwrap(), before);
    // a palette whose rows are another size is refused too
    std::fs::write(
        dir.path().join("small.gpl"),
        gpl::write(
            &PaletteSource {
                row_size: 4,
                rows: vec![vec![0, 1, 2, 3]],
            },
            "s",
        ),
    )
    .unwrap();
    let args = [
        "art",
        "palette",
        "set",
        "hero.png",
        "small.gpl",
        "--width",
        "16",
        "--height",
        "16",
    ];
    let (code, _, err) = ppu_bytes(dir.path(), &args, &px);
    assert_eq!(code, 2);
    assert!(err.contains("4 colours"), "{err}");
}

#[test]
fn the_report_gives_the_gpls_colour_count() {
    let p = pal();
    let dir = project(
        &png(&p, 16, 16, vec![1; 256]),
        &png(&p, 16, 16, vec![1; 256]),
    );
    let (_, out, _) = ppu(dir.path(), &["art", "check", "hero.png", "--json"], None);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["palette_colours"], 32);
    // the PLTE the art means is the .gpl itself, every entry its own
    assert_eq!(v["plte_own"], 32);
    let plte = v["plte"].as_array().unwrap();
    assert_eq!(plte.len(), 32, "{out}");
    let [r, g, b, _] = ppu_core::unpack_rgb15(p.rows[1][5]);
    assert_eq!(plte[21], serde_json::json!([r, g, b]));
    let (_, out, _) = ppu(
        dir.path(),
        &["art", "check", "art/other.png", "--json"],
        None,
    );
    let o: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert!(
        o.get("palette_colours").is_none() && o.get("plte").is_none(),
        "{out}"
    );
}

#[test]
fn palette_set_keeps_every_field_of_ppu_json_it_does_not_model() {
    let hero = png(&pal(), 16, 16, vec![0; 256]);
    let dir = project(&hero, &hero);
    let before: serde_json::Value = serde_json::from_str(
        r#"{"title":"t","tags":["a","b"],"files":["main.luau"],"sources":[
 {"name":"pal","kind":"palette","file":"pal.gpl","note":{"x":1}},
 {"name":"hero","kind":"bg","options":{"zeta":1,"palette":"pal","alpha":2},"file":"hero.png","owner":"me"},
 {"name":"other","kind":"bg","file":"art/other.png"}],"zz":true}"#,
    )
    .unwrap();
    std::fs::write(
        dir.path().join("ppu.json"),
        serde_json::to_string_pretty(&before).unwrap(),
    )
    .unwrap();
    std::fs::write(dir.path().join("same.gpl"), gpl::write(&pal(), "same")).unwrap();
    let args = [
        "art", "palette", "set", "hero.png", "same.gpl", "--width", "16", "--height", "16",
    ];
    let (code, _, err) = ppu_bytes(dir.path(), &args, &[0u8; 256]);
    assert_eq!(code, 0, "{err}");
    let after: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.path().join("ppu.json")).unwrap())
            .unwrap();
    // only options.palette changed, and a palette source was added for same.gpl
    let mut want = before.clone();
    want["sources"][1]["options"]["palette"] = "same".into();
    want["sources"].as_array_mut().unwrap().push(serde_json::json!(
        {"name":"same","kind":"palette","file":"same.gpl","meta":after["sources"][3]["meta"].clone()}
    ));
    // value equality: every original key is still there; key order is not promised
    assert_eq!(after, want);
}

/// Every file under `dir`, by relative path.
fn snapshot(dir: &Path) -> std::collections::BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, at: &Path, out: &mut std::collections::BTreeMap<String, Vec<u8>>) {
        for e in std::fs::read_dir(at).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk(root, &p, out);
            } else {
                let rel = p.strip_prefix(root).unwrap().to_string_lossy().into_owned();
                out.insert(rel, std::fs::read(&p).unwrap());
            }
        }
    }
    let mut out = Default::default();
    walk(dir, dir, &mut out);
    out
}

fn manifest(dir: &Path) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(dir.join("ppu.json")).unwrap()).unwrap()
}

fn source(dir: &Path, name: &str) -> serde_json::Value {
    manifest(dir)["sources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == name)
        .unwrap_or_else(|| panic!("no source {name}"))
        .clone()
}

#[test]
fn art_new_makes_an_empty_png_and_its_source_that_pass_the_check_for_every_kind() {
    let blank = png(&pal(), 16, 16, vec![0; 256]);
    let dir = project(&blank, &blank);
    let cases: [(&str, &[&str], u32); 6] = [
        ("bgfour", &["--kind", "bg", "--size", "32x16"], 4),
        (
            "bgtwo",
            &["--kind", "bg", "--size", "16x16", "--bpp", "2"],
            2,
        ),
        (
            "bgeight",
            &["--kind", "bg", "--size", "16x16", "--bpp", "8"],
            8,
        ),
        ("sheet", &["--kind", "sheet", "--size", "16x24"], 4),
        (
            "sprites",
            &["--kind", "obj", "--size", "32x16", "--cell", "16"],
            4,
        ),
        ("track", &["--kind", "m7", "--size", "64x64"], 8),
    ];
    for (name, flags, bpp) in cases {
        let file = format!("art/{name}.png");
        let pal_name = format!("{name}_pal");
        let mut args = vec!["art", "new", file.as_str()];
        args.extend(flags);
        args.extend(["--new-palette", pal_name.as_str()]);
        let (code, out, err) = ppu(dir.path(), &args, None);
        assert_eq!(code, 0, "{name}: {out}{err}");
        // one line per file written: the PNG, the .gpl, ppu.json
        assert_eq!(out.lines().count(), 3, "{name}: {out}");
        let (code, out, _) = ppu(dir.path(), &["art", "check", file.as_str(), "--json"], None);
        assert_eq!(code, 0, "{name}: {out}");
        let s = source(dir.path(), name);
        assert_eq!(s["options"]["bit_depth"], bpp, "{name}");
        assert_eq!(s["options"]["palette"], pal_name.as_str(), "{name}");
        assert_eq!(s["file"], file.as_str());
        let listed = source(dir.path(), &pal_name);
        assert_eq!(listed["kind"], "palette");
        assert_eq!(listed["file"], format!("palettes/{pal_name}.gpl").as_str());
        assert!(dir
            .path()
            .join(format!("palettes/{pal_name}.gpl"))
            .is_file());
    }
    assert_eq!(source(dir.path(), "sprites")["options"]["cell_size"], 16);
    assert!(source(dir.path(), "bgfour")["options"]
        .get("cell_size")
        .is_none());
    // every pixel is index 0
    let new = std::fs::read(dir.path().join("art/bgfour.png")).unwrap();
    let ppu_core::render::DecodedPng::Indexed(ip) = ppu_core::render::decode_png(&new).unwrap()
    else {
        panic!("not indexed")
    };
    assert_eq!((ip.image.width, ip.image.height), (32, 16));
    assert!(ip.image.pixels.iter().all(|&p| p == 0));
}

#[test]
fn art_new_names_a_listed_gpl_and_lists_an_unlisted_one() {
    let blank = png(&pal(), 16, 16, vec![0; 256]);
    let dir = project(&blank, &blank);
    let (code, out, err) = ppu(
        dir.path(),
        &[
            "art",
            "new",
            "fresh.png",
            "--kind",
            "bg",
            "--size",
            "16x16",
            "--palette",
            "pal.gpl",
        ],
        None,
    );
    assert_eq!(code, 0, "{out}{err}");
    // the PNG and ppu.json: pal.gpl was already listed, so nothing else is written
    assert_eq!(out.lines().count(), 2, "{out}");
    assert_eq!(source(dir.path(), "fresh")["options"]["palette"], "pal");
    assert_eq!(ppu(dir.path(), &["art", "check", "fresh.png"], None).0, 0);

    std::fs::write(dir.path().join("extra.gpl"), gpl::write(&pal(), "extra")).unwrap();
    let (code, out, err) = ppu(
        dir.path(),
        &[
            "art",
            "new",
            "art/two.png",
            "--kind",
            "bg",
            "--size",
            "8x8",
            "--palette",
            "extra.gpl",
        ],
        None,
    );
    assert_eq!(code, 0, "{out}{err}");
    assert_eq!(source(dir.path(), "two")["options"]["palette"], "extra");
    assert_eq!(source(dir.path(), "extra")["file"], "extra.gpl");
    assert_eq!(ppu(dir.path(), &["art", "check", "art/two.png"], None).0, 0);
    // a palette sync reaches the new art like any other
    assert_eq!(
        ppu(dir.path(), &["art", "palette", "sync", "extra.gpl"], None).0,
        0
    );
}

#[test]
fn art_new_refusals_exit_2_and_leave_the_project_byte_identical() {
    let blank = png(&pal(), 16, 16, vec![0; 256]);
    let dir = project(&blank, &blank);
    std::fs::write(
        dir.path().join("four.gpl"),
        gpl::write(
            &PaletteSource {
                row_size: 4,
                rows: vec![vec![0, 1, 2, 3]],
            },
            "four",
        ),
    )
    .unwrap();
    std::fs::write(dir.path().join("hero2.gpl"), gpl::write(&pal(), "hero2")).unwrap();
    std::fs::create_dir(dir.path().join("palettes")).unwrap();
    std::fs::write(dir.path().join("palettes/taken.gpl"), "x").unwrap();
    let outside = tempfile::tempdir().unwrap();
    let outside_png = outside.path().join("o.png");
    let outside_png = outside_png.to_str().unwrap();
    let cases: [(&[&str], &str); 19] = [
        (
            &[
                "art",
                "new",
                "hero.png",
                "--kind",
                "bg",
                "--size",
                "8x8",
                "--palette",
                "pal.gpl",
            ],
            "already exists",
        ),
        (
            &[
                "art",
                "new",
                "other.png",
                "--kind",
                "bg",
                "--size",
                "8x8",
                "--palette",
                "pal.gpl",
            ],
            "already has a source named \"other\"",
        ),
        (
            &[
                "art",
                "new",
                "a.png",
                "--kind",
                "bg",
                "--size",
                "8x8",
                "--new-palette",
                "pal",
            ],
            "already has a source named \"pal\"",
        ),
        (
            &[
                "art",
                "new",
                "a.png",
                "--kind",
                "bg",
                "--size",
                "8x8",
                "--new-palette",
                "taken",
            ],
            "already exists",
        ),
        (
            &[
                "art",
                "new",
                "a.png",
                "--kind",
                "bg",
                "--size",
                "12x8",
                "--palette",
                "pal.gpl",
            ],
            "multiple of 8",
        ),
        (
            &[
                "art",
                "new",
                "a.png",
                "--kind",
                "bg",
                "--size",
                "0x8",
                "--palette",
                "pal.gpl",
            ],
            "positive multiple",
        ),
        (
            &[
                "art",
                "new",
                "a.png",
                "--kind",
                "obj",
                "--size",
                "24x16",
                "--cell",
                "16",
                "--palette",
                "pal.gpl",
            ],
            "multiple of 16",
        ),
        (
            &[
                "art",
                "new",
                "a.png",
                "--kind",
                "m7",
                "--size",
                "8x8",
                "--bpp",
                "4",
                "--palette",
                "pal.gpl",
            ],
            "--bpp 4 is not legal for a m7",
        ),
        (
            &[
                "art",
                "new",
                "a.png",
                "--kind",
                "obj",
                "--size",
                "8x8",
                "--bpp",
                "2",
                "--palette",
                "pal.gpl",
            ],
            "--bpp 2 is not legal for a obj",
        ),
        (
            &[
                "art",
                "new",
                "a.png",
                "--kind",
                "bg",
                "--size",
                "8x8",
                "--bpp",
                "3",
                "--palette",
                "pal.gpl",
            ],
            "--bpp 3 is not legal",
        ),
        (
            &[
                "art",
                "new",
                "a.png",
                "--kind",
                "bg",
                "--size",
                "8x8",
                "--cell",
                "16",
                "--palette",
                "pal.gpl",
            ],
            "--cell only goes with --kind obj",
        ),
        (
            &["art", "new", "a.png", "--kind", "bg", "--size", "8x8"],
            "exactly one of",
        ),
        (
            &[
                "art",
                "new",
                "a.png",
                "--kind",
                "bg",
                "--size",
                "8x8",
                "--palette",
                "pal.gpl",
                "--new-palette",
                "n",
            ],
            "exactly one of",
        ),
        (
            &[
                "art",
                "new",
                "a.png",
                "--kind",
                "bg",
                "--size",
                "8x8",
                "--palette",
                "four.gpl",
            ],
            "its rows are 4 colours",
        ),
        (
            &[
                "art",
                "new",
                "a b.png",
                "--kind",
                "bg",
                "--size",
                "8x8",
                "--palette",
                "pal.gpl",
            ],
            "unsafe source name",
        ),
        (
            &[
                "art",
                "new",
                "a.png",
                "--kind",
                "bg",
                "--size",
                "8x8",
                "--new-palette",
                "../evil",
            ],
            "unsafe palette name",
        ),
        (
            &[
                "art",
                "new",
                "a.png",
                "--kind",
                "m7",
                "--size",
                "100000x100000",
                "--new-palette",
                "n",
            ],
            "no art is larger than 1024 px",
        ),
        (
            &[
                "art",
                "new",
                "a.png",
                "--kind",
                "bg",
                "--size",
                "8x8",
                "--new-palette",
                "a",
            ],
            "new palette's name must differ from the art's",
        ),
        (
            &[
                "art",
                "new",
                "a.png",
                "--kind",
                "palette",
                "--size",
                "8x8",
                "--palette",
                "pal.gpl",
            ],
            "--kind must be bg, sheet, obj or m7",
        ),
    ];
    let before = snapshot(dir.path());
    for (args, phrase) in cases {
        let (code, out, err) = ppu(dir.path(), args, None);
        assert_eq!(code, 2, "{args:?}: {out}{err}");
        assert!(err.contains(phrase), "{args:?}: {err}");
        assert_eq!(snapshot(dir.path()), before, "{args:?} changed the project");
    }
    // a PNG outside any project, and a .gpl that would collide with the art's own name
    let (code, out, _) = ppu(
        dir.path(),
        &[
            "art",
            "new",
            outside_png,
            "--kind",
            "bg",
            "--size",
            "8x8",
            "--palette",
            "pal.gpl",
            "--json",
        ],
        None,
    );
    assert_eq!(code, 2);
    assert_eq!(answer(&out)["reason"], "not_project", "{out}");
    assert!(!Path::new(outside_png).exists());
    let (code, _, err) = ppu(
        dir.path(),
        &[
            "art",
            "new",
            "hero2.png",
            "--kind",
            "bg",
            "--size",
            "8x8",
            "--palette",
            "hero2.gpl",
        ],
        None,
    );
    assert_eq!(code, 2, "{err}");
    assert_eq!(snapshot(dir.path()), before);
}

#[test]
fn art_new_keeps_the_fields_of_ppu_json_it_does_not_model() {
    let blank = png(&pal(), 16, 16, vec![0; 256]);
    let dir = project(&blank, &blank);
    let mut m = manifest(dir.path());
    m["future"] = serde_json::json!({"keep": [1, 2]});
    m["sources"][1]["note"] = "hand written".into();
    std::fs::write(
        dir.path().join("ppu.json"),
        serde_json::to_string(&m).unwrap(),
    )
    .unwrap();
    let (code, out, err) = ppu(
        dir.path(),
        &[
            "art",
            "new",
            "n.png",
            "--kind",
            "bg",
            "--size",
            "8x8",
            "--palette",
            "pal.gpl",
        ],
        None,
    );
    assert_eq!(code, 0, "{out}{err}");
    let after = manifest(dir.path());
    assert_eq!(after["future"], m["future"]);
    assert_eq!(after["sources"][1], m["sources"][1]);
    assert_eq!(after["sources"].as_array().unwrap().len(), 4);
}

fn editor_file(name: &str) -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("editors/aseprite")
            .join(name),
    )
    .unwrap()
}

#[test]
fn art_install_aseprite_writes_the_plugin_over_an_older_install() {
    let user = tempfile::tempdir().unwrap();
    let u = user.path().to_str().unwrap();
    let at = user.path().join("extensions/ppu-art");
    std::fs::create_dir_all(&at).unwrap();
    std::fs::write(at.join("snes-art.lua"), "old").unwrap();
    let (code, out, err) = ppu(
        user.path(),
        &["art", "install", "aseprite", "--dir", u],
        None,
    );
    assert_eq!(code, 0, "{out}{err}");
    for f in ["package.json", "snes-art.lua"] {
        assert_eq!(std::fs::read(at.join(f)).unwrap(), editor_file(f), "{f}");
    }
    assert!(out.contains(at.to_str().unwrap()), "{out}");
    assert!(out.contains("Restart Aseprite."), "{out}");
    assert!(out.contains("Give full trust to this script"), "{out}");
    assert!(!std::fs::read_dir(&at).unwrap().any(|e| e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .ends_with(".tmp")));
    // ASEPRITE_USER_FOLDER is the user folder when --dir is not given
    let other = tempfile::tempdir().unwrap();
    let mut c = Command::new(env!("CARGO_BIN_EXE_ppu"));
    let o = c
        .args(["art", "install", "aseprite"])
        .env("ASEPRITE_USER_FOLDER", other.path())
        .output()
        .unwrap();
    assert!(o.status.success());
    assert_eq!(
        std::fs::read(other.path().join("extensions/ppu-art/snes-art.lua")).unwrap(),
        editor_file("snes-art.lua")
    );
}

/// The entries (name, bytes) of a stored zip, read from its central directory.
fn zip_entries(z: &[u8]) -> Vec<(String, Vec<u8>)> {
    let u16_at = |i: usize| u16::from_le_bytes([z[i], z[i + 1]]) as usize;
    let u32_at = |i: usize| u16_at(i) | u16_at(i + 2) << 16;
    let eocd = z.len() - 22;
    assert_eq!(&z[eocd..eocd + 4], b"PK\x05\x06");
    let mut at = u32_at(eocd + 16);
    (0..u16_at(eocd + 10))
        .map(|_| {
            assert_eq!(&z[at..at + 4], b"PK\x01\x02");
            assert_eq!(u16_at(at + 10), 0, "stored");
            let (len, n, local) = (u32_at(at + 24), u16_at(at + 28), u32_at(at + 42));
            let name = String::from_utf8(z[at + 46..at + 46 + n].to_vec()).unwrap();
            let data = local + 30 + u16_at(local + 26);
            at += 46 + n;
            (name, z[data..data + len].to_vec())
        })
        .collect()
}

#[test]
fn art_install_pixelorama_writes_the_extension_zip_over_an_older_install() {
    let user = tempfile::tempdir().unwrap();
    let u = user.path().to_str().unwrap();
    let at = user.path().join("extensions");
    std::fs::create_dir_all(&at).unwrap();
    std::fs::write(at.join("SnesArt.zip"), "old").unwrap();
    let (code, out, err) = ppu(
        user.path(),
        &["art", "install", "pixelorama", "--dir", u],
        None,
    );
    assert_eq!(code, 0, "{out}{err}");
    let entries = zip_entries(&std::fs::read(at.join("SnesArt.zip")).unwrap());
    let names: Vec<&str> = entries.iter().map(|(n, _)| n.as_str()).collect();
    let want = vec![
        "extension.json",
        "main.gd",
        "main.tscn",
        "overlay.gd",
        "png.gd",
    ];
    assert_eq!(
        names,
        want.iter()
            .map(|f| format!("src/Extensions/SnesArt/{f}"))
            .collect::<Vec<_>>()
    );
    for (n, bytes) in &entries {
        let f = n.rsplit('/').next().unwrap();
        let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("editors/pixelorama/SnesArt")
            .join(f);
        assert_eq!(bytes, &std::fs::read(disk).unwrap(), "{n}");
    }
    assert!(
        out.contains("SnesArt.zip") && out.contains("Preferences > Extensions"),
        "{out}"
    );
    assert_eq!(
        std::fs::read_dir(&at).unwrap().count(),
        1,
        "no temp file left"
    );
    // XDG_DATA_HOME is the data folder when --dir is not given
    let other = tempfile::tempdir().unwrap();
    let o = Command::new(env!("CARGO_BIN_EXE_ppu"))
        .args(["art", "install", "pixelorama"])
        .env("XDG_DATA_HOME", other.path())
        .output()
        .unwrap();
    assert!(o.status.success());
    assert!(other
        .path()
        .join("pixelorama/extensions/SnesArt.zip")
        .exists());
}

#[test]
fn art_install_refuses_an_unknown_editor_by_name() {
    let user = tempfile::tempdir().unwrap();
    let u = user.path().to_str().unwrap();
    let (code, _, err) = ppu(user.path(), &["art", "install", "krita", "--dir", u], None);
    assert_eq!(code, 2);
    assert!(
        err.contains("unknown editor \"krita\"") && err.contains("aseprite, pixelorama"),
        "{err}"
    );
    assert!(snapshot(user.path()).is_empty());
}

/// A full-colour (RGBA) PNG: `colour(x, y)` per pixel.
fn rgba_png(w: u32, h: u32, mut colour: impl FnMut(u32, u32) -> [u8; 3]) -> Vec<u8> {
    let mut out = vec![];
    let mut enc = png::Encoder::new(&mut out, w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut data = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let [r, g, b] = colour(x, y);
            data.extend([r, g, b, 255]);
        }
    }
    enc.write_header().unwrap().write_image_data(&data).unwrap();
    out
}

/// Five hues in a diagonal pattern: a few distinct tiles and colours, none of them SNES-exact.
fn drawing(w: u32, h: u32) -> Vec<u8> {
    const HUES: [[u8; 3]; 5] = [
        [201, 40, 33],
        [38, 160, 77],
        [52, 71, 214],
        [233, 201, 60],
        [120, 120, 130],
    ];
    rgba_png(w, h, |x, y| HUES[((x / 4 + y / 8) % 5) as usize])
}

fn convert_project(name: &str, art: &[u8]) -> tempfile::TempDir {
    let blank = png(&pal(), 16, 16, vec![0; 256]);
    let dir = project(&blank, &blank);
    std::fs::write(dir.path().join("main.luau"), "function frame() end").unwrap();
    std::fs::write(dir.path().join(format!("art/{name}.png")), art).unwrap();
    dir
}

#[test]
fn check_json_reports_the_options_a_listed_full_colour_png_stores() {
    let dir = convert_project("pic", &drawing(32, 32));
    let manifest = std::fs::read_to_string(dir.path().join("ppu.json")).unwrap();
    std::fs::write(
        dir.path().join("ppu.json"),
        manifest.replace(
            r#"{"name":"other","kind":"bg","file":"art/other.png"}"#,
            r#"{"name":"other","kind":"obj","options":{"cell_size":16,"dither":"bayer","max_palettes":3},"file":"art/other.png"}"#,
        ),
    )
    .unwrap();
    std::fs::copy(
        dir.path().join("art/pic.png"),
        dir.path().join("art/other.png"),
    )
    .unwrap();
    let (code, out, err) = ppu(
        dir.path(),
        &["art", "check", "art/other.png", "--json"],
        None,
    );
    assert_eq!(code, 0, "{out}{err}");
    let j: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(j["path"], "quantize");
    // exactly what the entry stores: absent options stay absent
    assert_eq!(
        j["options"],
        serde_json::json!({"cell_size": 16, "dither": "bayer", "max_palettes": 3})
    );
}

#[test]
fn art_convert_makes_snes_art_of_a_full_colour_png_for_every_kind() {
    use base64::Engine;
    let cases: [(&str, &[&str], u32, u32); 5] = [
        ("bg", &["--kind", "bg"], 32, 32),
        ("two", &["--kind", "bg", "--bpp", "2"], 32, 32),
        ("sheet", &["--kind", "sheet"], 32, 32),
        ("obj", &["--kind", "obj", "--cell", "16"], 32, 32),
        ("m7", &["--kind", "m7"], 64, 64),
    ];
    for (name, flags, w, h) in cases {
        let original = drawing(w, h);
        let dir = convert_project(name, &original);
        let file = format!("art/{name}.png");
        let mut args = vec!["art", "convert", file.as_str()];
        args.extend(flags);
        args.push("--json");
        let (code, out, err) = ppu(dir.path(), &args, None);
        assert_eq!(code, 0, "{name}: {out}{err}");
        let j: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(j["ok"], true, "{name}: {out}");
        assert!(j["colours"].as_u64().unwrap() >= 5, "{name}: {out}");
        for b in ["tiles", "palettes"] {
            assert!(
                j[b]["used"].as_u64().unwrap() >= 1
                    && j[b]["used"].as_u64() <= j[b]["max"].as_u64(),
                "{name} {b}: {out}"
            );
        }
        let pal_name = format!("{name}_pal");
        let s = source(dir.path(), name);
        assert_eq!(s["options"]["palette"], pal_name.as_str(), "{name}");
        assert_eq!(s["file"], file.as_str());
        assert_eq!(source(dir.path(), &pal_name)["kind"], "palette");
        assert_eq!(j["files"].as_array().unwrap().len(), 3, "{name}: {out}");

        // the original is kept, byte for byte, where the command says
        let backup = std::path::PathBuf::from(j["backup"].as_str().unwrap());
        assert!(backup.starts_with(dir.path().join(".ppu-backup")), "{name}");
        assert_eq!(std::fs::read(&backup).unwrap(), original, "{name}");

        // it now passes the check on the keep path
        let (code, out, _) = ppu(dir.path(), &["art", "check", file.as_str(), "--json"], None);
        assert_eq!(code, 0, "{name}: {out}");
        assert!(out.contains(r#""path":"keep""#), "{name}: {out}");

        // the written PNG imports to what the site makes of the original, same options
        let opts: ppu_core::ConvertOptions = serde_json::from_value(s["options"].clone()).unwrap();
        let kind: ppu_core::SourceKind = s["kind"].as_str().unwrap().parse().unwrap();
        let site = ppu_core::convert_png_source(kind, &opts, &original, None)
            .unwrap()
            .0
            .encode();
        let written = std::fs::read(dir.path().join(&file)).unwrap();
        let again = ppu_core::convert_png_source(kind, &opts, &written, None)
            .unwrap()
            .0
            .encode();
        assert_eq!(again, site, "{name}");

        // and it is what a sync sends
        let packed: serde_json::Value =
            serde_json::from_str(&ppu_cli::pack(dir.path()).unwrap()).unwrap();
        let sent = packed["sources"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["name"] == name)
            .unwrap();
        assert_eq!(
            base64::engine::general_purpose::STANDARD
                .decode(sent["payload"].as_str().unwrap())
                .unwrap(),
            site,
            "{name}"
        );
    }
}

#[test]
fn art_convert_uses_a_listed_entrys_kind_and_options_and_keeps_its_fields() {
    let original = drawing(32, 32);
    let dir = convert_project("photo", &original);
    let mut m = manifest(dir.path());
    m["sources"].as_array_mut().unwrap().push(serde_json::json!(
        {"name":"photo","kind":"bg","options":{"bit_depth":2},"file":"art/photo.png","note":"kept"}
    ));
    std::fs::write(dir.path().join("ppu.json"), m.to_string()).unwrap();
    let before = snapshot(dir.path());
    // --kind must match the entry; a flag the kind ignores (--cell on a bg) is refused
    for (flags, want) in [
        (&["--kind", "obj"][..], "listed as a bg source"),
        (&["--cell", "16"], "--cell only goes with --kind obj"),
        (
            &["--cell-height", "16"],
            "--cell-height does nothing for a bg",
        ),
    ] {
        let mut args = vec!["art", "convert", "art/photo.png"];
        args.extend(flags);
        let (code, _, err) = ppu(dir.path(), &args, None);
        assert_eq!(code, 2, "{err}");
        assert!(err.contains(want), "{err}");
        assert_eq!(snapshot(dir.path()), before);
    }
    let (code, out, err) = ppu(dir.path(), &["art", "convert", "art/photo.png"], None);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("kept the original in"), "{out}");
    let s = source(dir.path(), "photo");
    assert_eq!(s["options"]["bit_depth"], 2);
    assert_eq!(s["options"]["palette"], "photo_pal");
    assert_eq!(s["note"], "kept");
    let (code, out, _) = ppu(dir.path(), &["art", "check", "art/photo.png"], None);
    assert_eq!(code, 0, "{out}");
    // it is SNES art now: converting it again is refused, nothing changes
    let after = snapshot(dir.path());
    let (code, _, err) = ppu(dir.path(), &["art", "convert", "art/photo.png"], None);
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("already SNES art"), "{err}");
    assert_eq!(snapshot(dir.path()), after);
}

#[test]
fn art_convert_says_when_art_leaves_the_palette_it_named() {
    let dir = convert_project("photo", &drawing(32, 32));
    let mut m = manifest(dir.path());
    m["sources"].as_array_mut().unwrap().push(serde_json::json!(
        {"name":"photo","kind":"bg","options":{"palette":"pal"},"file":"art/photo.png"}
    ));
    std::fs::write(dir.path().join("ppu.json"), m.to_string()).unwrap();
    let (code, out, err) = ppu(dir.path(), &["art", "convert", "art/photo.png"], None);
    assert_eq!(code, 0, "{out}{err}");
    assert!(
        out.contains(r#"photo now uses the palette photo_pal, not pal: code that loads it with palette = "pal" must name "photo_pal"."#),
        "{out}"
    );
}

#[test]
fn art_convert_refusals_leave_every_file_byte_identical() {
    // too many tiles: a noise bg has more unique tiles than a bg holds; the importer refuses,
    // exit 1 with its message, and --json says so
    let mut seed = 0x2545F491u32;
    let noise = rgba_png(264, 264, |_, _| {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let b = seed.to_be_bytes();
        [b[0], b[1], b[2]]
    });
    let dir = convert_project("noise", &noise);
    let before = snapshot(dir.path());
    let (code, out, err) = ppu(
        dir.path(),
        &["art", "convert", "art/noise.png", "--kind", "bg"],
        None,
    );
    assert_eq!(code, 1, "{out}{err}");
    assert!(err.starts_with("ppu: "), "{err}");
    assert_eq!(snapshot(dir.path()), before);
    let (code, out, _) = ppu(
        dir.path(),
        &["art", "convert", "art/noise.png", "--kind", "bg", "--json"],
        None,
    );
    assert_eq!(code, 1, "{out}");
    let j: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(j["ok"], false);
    assert!(!j["error"].as_str().unwrap().is_empty());
    assert_eq!(snapshot(dir.path()), before);

    // usage-class refusals exit 2 and write nothing either
    let dir = convert_project("ok", &drawing(32, 32));
    std::fs::create_dir(dir.path().join("palettes")).unwrap();
    std::fs::write(dir.path().join("palettes/taken.gpl"), "GIMP Palette\n").unwrap();
    let mut m = manifest(dir.path());
    m["sources"].as_array_mut().unwrap().push(serde_json::json!(
        {"name":"masked","kind":"m7","options":{"extbg":true},"file":"art/ok.png","priority_file":"art/other.png"}
    ));
    let masked = m.to_string();
    let plain = std::fs::read_to_string(dir.path().join("ppu.json")).unwrap();
    let cases: [(&str, &[&str], &str); 6] = [
        ("art/ok.png", &[], "--kind"),
        (
            "art/ok.png",
            &["--kind", "bg", "--new-palette", "taken"],
            "already exists",
        ),
        (
            "art/ok.png",
            &["--kind", "bg", "--new-palette", "pal"],
            "already has a source",
        ),
        (
            "art/ok.png",
            &["--kind", "bg", "--new-palette", "bad name"],
            "unsafe",
        ),
        ("art/ok.png", &["--kind", "bg", "--bpp", "3"], "--bpp 3"),
        ("art/ok.png", &["--kind", "obj", "--cell", "9"], "--cell"),
    ];
    for (file, flags, phrase) in cases {
        let before = snapshot(dir.path());
        let mut args = vec!["art", "convert", file];
        args.extend(flags);
        let (code, _, err) = ppu(dir.path(), &args, None);
        assert_eq!(code, 2, "{flags:?}: {err}");
        assert!(err.contains(phrase), "{flags:?}: {err}");
        assert_eq!(snapshot(dir.path()), before, "{flags:?}");
    }
    // a priority_file entry is refused by name
    std::fs::write(dir.path().join("ppu.json"), masked).unwrap();
    let before = snapshot(dir.path());
    let (code, _, err) = ppu(dir.path(), &["art", "convert", "art/ok.png"], None);
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("priority_file"), "{err}");
    assert_eq!(snapshot(dir.path()), before);
    std::fs::write(dir.path().join("ppu.json"), plain).unwrap();
}

#[test]
fn art_convert_never_overwrites_an_earlier_backup() {
    let original = drawing(32, 32);
    let dir = convert_project("again", &original);
    std::fs::create_dir_all(dir.path().join(".ppu-backup/convert-1/art")).unwrap();
    std::fs::write(
        dir.path().join(".ppu-backup/convert-1/art/again.png"),
        b"old",
    )
    .unwrap();
    let (code, out, err) = ppu(
        dir.path(),
        &[
            "art",
            "convert",
            "art/again.png",
            "--kind",
            "bg",
            "--new-palette",
            "mine",
        ],
        None,
    );
    assert_eq!(code, 0, "{out}{err}");
    assert_eq!(
        std::fs::read(dir.path().join(".ppu-backup/convert-1/art/again.png")).unwrap(),
        b"old"
    );
    assert_eq!(
        std::fs::read(dir.path().join(".ppu-backup/convert-2/art/again.png")).unwrap(),
        original
    );
    assert!(out.contains("convert-2"), "{out}");
    assert_eq!(source(dir.path(), "again")["options"]["palette"], "mine");
    assert!(dir.path().join("palettes/mine.gpl").is_file());
}

#[test]
fn art_convert_flags_are_stored_and_make_the_same_payload_again() {
    let gradient = rgba_png(64, 64, |x, y| [(x * 4) as u8, (y * 4) as u8, 128]);
    let run = |name: &str, flags: &[&str]| {
        let dir = convert_project(name, &gradient);
        let file = format!("art/{name}.png");
        let mut args = vec!["art", "convert", file.as_str()];
        args.extend(flags);
        let (code, out, err) = ppu(dir.path(), &args, None);
        (dir, file, code, format!("{out}{err}"))
    };
    let payload = |dir: &Path, file: &str, name: &str| {
        let s = source(dir, name);
        let opts: ppu_core::ConvertOptions = serde_json::from_value(s["options"].clone()).unwrap();
        let kind: ppu_core::SourceKind = s["kind"].as_str().unwrap().parse().unwrap();
        let written = std::fs::read(dir.join(file)).unwrap();
        let site = ppu_core::convert_png_source(kind, &opts, &gradient, None)
            .unwrap()
            .0
            .encode();
        let again = ppu_core::convert_png_source(kind, &opts, &written, None)
            .unwrap()
            .0
            .encode();
        assert_eq!(
            again, site,
            "{name}: the stored options make the same payload"
        );
        again
    };
    let cases: [(&str, &[&str], &[(&str, serde_json::Value)]); 5] = [
        ("none", &["--kind", "bg"], &[]),
        (
            "bay",
            &[
                "--kind",
                "bg",
                "--dither",
                "bayer",
                "--dither-strength",
                "80",
            ],
            &[("dither", "bayer".into()), ("dither_strength", 80.into())],
        ),
        (
            "dif",
            &[
                "--kind",
                "bg",
                "--dither",
                "diffusion",
                "--max-palettes",
                "2",
                "--alpha-threshold",
                "200",
            ],
            &[
                ("dither", "diffusion".into()),
                ("max_palettes", 2.into()),
                ("alpha_threshold", 200.into()),
            ],
        ),
        (
            "sheet",
            &[
                "--kind",
                "sheet",
                "--dither",
                "bayer",
                "--max-palettes",
                "3",
            ],
            &[("dither", "bayer".into()), ("max_palettes", 3.into())],
        ),
        (
            "obj",
            &[
                "--kind",
                "obj",
                "--cell",
                "16",
                "--cell-height",
                "32",
                "--dither",
                "bayer",
            ],
            &[
                ("cell_size", 16.into()),
                ("cell_height", 32.into()),
                ("dither", "bayer".into()),
            ],
        ),
    ];
    let mut payloads = std::collections::BTreeMap::new();
    for (name, flags, stored) in cases {
        let (dir, file, code, out) = run(name, flags);
        assert_eq!(code, 0, "{name}: {out}");
        let o = source(dir.path(), name)["options"].clone();
        for (k, v) in stored {
            assert_eq!(o[*k], *v, "{name}: {k} stored");
        }
        payloads.insert(name, payload(dir.path(), &file, name));
        let (code, out, _) = ppu(dir.path(), &["art", "check", file.as_str()], None);
        assert_eq!(code, 0, "{name}: {out}");
    }
    assert_ne!(payloads["none"], payloads["bay"], "dither changes the art");
    assert_ne!(payloads["bay"], payloads["dif"]);
    // a flag overrides a listed entry's option and is stored beside the rest
    let dir = convert_project("hero", &gradient);
    let mut m = manifest(dir.path());
    m["sources"].as_array_mut().unwrap().push(serde_json::json!(
        {"name":"hero","kind":"bg","options":{"bit_depth":4,"dither":"bayer","note":"x"},"file":"art/hero.png"}
    ));
    std::fs::write(dir.path().join("ppu.json"), m.to_string()).unwrap();
    let (code, out, err) = ppu(
        dir.path(),
        &[
            "art",
            "convert",
            "art/hero.png",
            "--dither",
            "none",
            "--bpp",
            "2",
        ],
        None,
    );
    assert_eq!(code, 0, "{out}{err}");
    let o = source(dir.path(), "hero")["options"].clone();
    assert_eq!(
        (
            o["dither"].as_str(),
            o["bit_depth"].as_u64(),
            o["note"].as_str()
        ),
        (Some("none"), Some(2), Some("x"))
    );
    payload(dir.path(), "art/hero.png", "hero");
}

#[test]
fn art_convert_refuses_flags_the_kind_ignores_or_out_of_range_and_writes_nothing() {
    let gradient = rgba_png(64, 64, |x, y| [(x * 4) as u8, (y * 4) as u8, 128]);
    let dir = convert_project("g", &gradient);
    let before = snapshot(dir.path());
    for (flags, want) in [
        (
            &["--kind", "m7", "--dither", "bayer"][..],
            "--dither does nothing for a m7",
        ),
        (
            &["--kind", "m7", "--max-palettes", "2"],
            "--max-palettes does nothing for a m7",
        ),
        (
            &["--kind", "m7", "--alpha-threshold", "9"],
            "--alpha-threshold does nothing for a m7",
        ),
        (
            &["--kind", "m7", "--extbg"],
            "--extbg needs a Mode 7 priority mask",
        ),
        (
            &["--kind", "obj", "--max-palettes", "2"],
            "--max-palettes does nothing for a obj",
        ),
        (
            &["--kind", "bg", "--tile-size", "16"],
            "16x16 tile import is not implemented: the converter imports 8x8",
        ),
        (
            &["--kind", "obj", "--tile-size", "16"],
            "--tile-size does nothing for a obj",
        ),
        (
            &["--kind", "sheet", "--tile-size", "16"],
            "--tile-size does nothing for a sheet",
        ),
        (
            &["--kind", "bg", "--cell-height", "16"],
            "--cell-height does nothing for a bg",
        ),
        (
            &["--kind", "bg", "--cell", "16"],
            "--cell only goes with --kind obj",
        ),
        (
            &["--kind", "bg", "--dither", "sparkle"],
            "--dither must be none, bayer or diffusion",
        ),
        (
            &["--kind", "bg", "--dither-strength", "101"],
            "--dither-strength must be 0 to 100",
        ),
        (
            &["--kind", "bg", "--alpha-threshold", "256"],
            "--alpha-threshold must be 0 to 255",
        ),
        (
            &["--kind", "bg", "--max-palettes", "0"],
            "--max-palettes must be 1 to 8",
        ),
        (
            &["--kind", "bg", "--max-palettes", "9"],
            "--max-palettes must be 1 to 8",
        ),
        (
            &["--kind", "bg", "--tile-size", "12"],
            "--tile-size must be 8 or 16",
        ),
        (
            &["--kind", "obj", "--cell-height", "12"],
            "--cell-height must be a multiple of 8",
        ),
        (&["--kind", "bg", "--bogus", "1"], "usage"),
    ] {
        let mut args = vec!["art", "convert", "art/g.png"];
        args.extend(flags);
        let (code, out, err) = ppu(dir.path(), &args, None);
        assert_eq!(code, 2, "{flags:?}: {out}{err}");
        assert!(err.contains(want), "{flags:?}: {err}");
        assert_eq!(snapshot(dir.path()), before, "{flags:?}");
    }
}

#[test]
fn art_convert_refuses_a_picture_whose_size_would_change_and_writes_nothing() {
    // m7 past 1024 px and bg past 512 px are cropped by the importer
    for (name, kind, w, h) in [("m7big", "m7", 1100, 16), ("bgbig", "bg", 600, 16)] {
        let dir = convert_project(name, &drawing(w, h));
        let before = snapshot(dir.path());
        let file = format!("art/{name}.png");
        let (code, out, err) = ppu(dir.path(), &["art", "convert", &file, "--kind", kind], None);
        assert_eq!(code, 1, "{name}: {out}{err}");
        // bg past 512 px is named by the importer's own crop note, the others by both sizes
        let named = err.contains(&format!("{w}x{h} in")) || err.contains("larger than 512 px");
        assert!(named, "{name}: {err}");
        assert_eq!(snapshot(dir.path()), before, "{name}");
    }
}

#[test]
fn art_convert_prints_the_converters_notes_and_puts_them_in_json() {
    let gradient = rgba_png(64, 64, |x, y| [(x * 4) as u8, (y * 4) as u8, 128]);
    let dir = convert_project("g", &gradient);
    let flags = [
        "art",
        "convert",
        "art/g.png",
        "--kind",
        "bg",
        "--max-palettes",
        "1",
    ];
    let (code, out, err) = ppu(dir.path(), &flags, None);
    assert_eq!(code, 0, "{out}{err}");
    let lines: Vec<&str> = out.lines().filter(|l| l.starts_with("note: ")).collect();
    let dir2 = convert_project("g", &gradient);
    let mut args = flags.to_vec();
    args.push("--json");
    let (_, out2, _) = ppu(dir2.path(), &args, None);
    let j: serde_json::Value = serde_json::from_str(&out2).unwrap();
    let notes = j["notes"].as_array().unwrap();
    assert_eq!(notes.len(), lines.len(), "{out}");
    assert!(
        !notes.is_empty(),
        "a one-row gradient must merge colours: {out2}"
    );
}

#[test]
fn art_convert_palette_note_names_the_max_palettes_cap() {
    // Three 16 px bands, each its own two colours: six colours fit two 2bpp rows of three, but no
    // band's pair can share a row with another's, so three rows are wanted and two allowed.
    let pairs = [
        [[255, 0, 0], [128, 0, 0]],
        [[0, 255, 0], [0, 128, 0]],
        [[0, 0, 255], [0, 0, 128]],
    ];
    let quads = rgba_png(48, 16, |x, y| {
        pairs[(x / 16) as usize][((x + y) % 2) as usize]
    });
    let dir = convert_project("q", &quads);
    let flags = [
        "art",
        "convert",
        "art/q.png",
        "--kind",
        "bg",
        "--bpp",
        "2",
        "--max-palettes",
        "2",
    ];
    let (code, out, err) = ppu(dir.path(), &flags, None);
    assert_eq!(code, 0, "{out}{err}");
    // The note names the cap this run used, not the hardware's 8.
    assert!(out.contains("palette rows, but 2 fit"), "{out}");
}

#[test]
fn art_convert_runs_from_another_directory_than_the_project_root() {
    let dir = convert_project("g", &drawing(32, 32));
    let sub = dir.path().join("art");
    let (code, out, err) = ppu(&sub, &["art", "convert", "g.png", "--kind", "bg"], None);
    assert_eq!(code, 0, "{out}{err}");
    assert!(dir.path().join("palettes/g_pal.gpl").exists());
    assert_eq!(source(dir.path(), "g")["options"]["palette"], "g_pal");
    assert!(dir.path().join(".ppu-backup").exists());
}

#[test]
fn art_convert_of_an_off_grid_size_converts_and_keeps_the_size() {
    // the importer pads to whole tiles internally, but the PNG it renders is the picture's own size
    let dir = convert_project("off", &drawing(30, 20));
    let (code, out, err) = ppu(
        dir.path(),
        &["art", "convert", "art/off.png", "--kind", "bg"],
        None,
    );
    assert_eq!(code, 0, "{out}{err}");
    let dec = png::Decoder::new(std::fs::File::open(dir.path().join("art/off.png")).unwrap());
    let info = dec.read_info().unwrap();
    assert_eq!((info.info().width, info.info().height), (30, 20));
    let (code, out, err) = ppu(dir.path(), &["art", "check", "art/off.png"], None);
    assert_eq!(code, 0, "{out}{err}");
}

fn answer(out: &str) -> serde_json::Value {
    serde_json::from_str(out).unwrap_or_else(|e| panic!("not one JSON answer ({e}): {out:?}"))
}

#[test]
fn a_refusal_under_json_carries_a_reason_code_and_never_depends_on_its_message() {
    let p = pal();
    let good = png(&p, 16, 16, vec![1; 256]);
    let dir = project(&good, &good);
    let stray = tempfile::tempdir().unwrap();
    std::fs::write(stray.path().join("a.png"), &good).unwrap();
    std::fs::write(dir.path().join("unlisted.png"), &good).unwrap();
    std::fs::write(dir.path().join("loose.gpl"), "x").unwrap();
    let wrong_len = ["--stdin", "--width", "16", "--height", "16"];
    let cases: [(&Path, Vec<&str>, i32, &str); 7] = [
        (stray.path(), vec!["check", "a.png"], 2, "not_project"),
        (dir.path(), vec!["check", "unlisted.png"], 2, "not_listed"),
        (
            dir.path(),
            vec!["palette", "sync", "loose.gpl"],
            2,
            "not_listed",
        ),
        (dir.path(), vec!["check"], 2, "usage"),
        (
            dir.path(),
            [&["check", "hero.png"][..], &wrong_len].concat(),
            2,
            "error",
        ),
        (dir.path(), vec!["frobnicate"], 2, "usage"),
        (dir.path(), vec!["check", "hero.png", "--stdin"], 2, "usage"),
    ];
    for (cwd, args, code, reason) in cases {
        let mut full = vec!["art"];
        full.extend(args.iter());
        full.push("--json");
        let (got, out, err) = ppu(cwd, &full, Some(&[0; 3]));
        let v = answer(&out);
        assert_eq!(
            (got, v["ok"].clone(), v["protocol"].clone()),
            (code, false.into(), 1.into()),
            "{full:?}: {out}{err}"
        );
        assert_eq!(v["reason"], reason, "{full:?}: {out}");
        assert!(!v["error"].as_str().unwrap().is_empty(), "{out}");
    }
    // two sources on one PNG
    let json = std::fs::read_to_string(dir.path().join("ppu.json")).unwrap();
    std::fs::write(
        dir.path().join("ppu.json"),
        json.replace(
            r#"{"name":"other","kind":"bg","file":"art/other.png"}"#,
            r#"{"name":"again","kind":"obj","file":"hero.png"}"#,
        ),
    )
    .unwrap();
    let (code, out, _) = ppu(dir.path(), &["art", "check", "hero.png", "--json"], None);
    assert_eq!(
        (code, answer(&out)["reason"].clone()),
        (2, "listed_twice".into()),
        "{out}"
    );
    // an importer refusal
    let big = convert_project("bgbig", &drawing(600, 16));
    let (code, out, _) = ppu(
        big.path(),
        &["art", "convert", "art/bgbig.png", "--kind", "bg", "--json"],
        None,
    );
    assert_eq!(
        (code, answer(&out)["reason"].clone()),
        (1, "refused".into()),
        "{out}"
    );
}

#[test]
fn palette_set_json_reports_unmatched_colours_as_a_list_and_a_fit_as_hex_pixels() {
    let hero = png(&pal(), 16, 16, vec![0; 256]);
    let dir = project(&hero, &hero);
    let mut lacking = pal();
    lacking.rows[0][1] = 0x7fff;
    std::fs::write(dir.path().join("new.gpl"), gpl::write(&lacking, "new")).unwrap();
    let mut px = vec![0u8; 256];
    px[0] = 1;
    let args = [
        "art", "palette", "set", "hero.png", "new.gpl", "--width", "16", "--height", "16", "--json",
    ];
    let (code, out, _) = ppu_bytes(dir.path(), &args, &px);
    let v = answer(std::str::from_utf8(&out).unwrap());
    assert_eq!(
        (code, v["reason"].clone(), v["protocol"].clone()),
        (1, "unmatched".into(), 1.into())
    );
    assert!(
        v["unmatched"]
            .as_array()
            .unwrap()
            .iter()
            .any(|l| l.as_str().unwrap().contains("tile 0,0")),
        "{v}"
    );
    // a palette that fits: the index bytes come back as lowercase hex
    let p = pal();
    let swapped = PaletteSource {
        row_size: 16,
        rows: vec![p.rows[1].clone(), p.rows[0].clone()],
    };
    std::fs::write(dir.path().join("swap.gpl"), gpl::write(&swapped, "swap")).unwrap();
    let args = [
        "art", "palette", "set", "hero.png", "swap.gpl", "--width", "16", "--height", "16",
        "--json",
    ];
    let (code, out, err) = ppu_bytes(dir.path(), &args, &px);
    let v = answer(std::str::from_utf8(&out).unwrap());
    assert_eq!(code, 0, "{err}");
    let hex = v["pixels"].as_str().unwrap();
    assert_eq!((hex.len(), &hex[..4]), (512, "1110"), "{v}");
}

#[test]
fn every_art_command_answers_json_with_the_protocol_and_its_fields() {
    let p = pal();
    let hero = png(&p, 16, 16, vec![0; 256]);
    let dir = project(&hero, &hero);
    let d = dir.path();
    let proto = |out: &str| {
        let v = answer(out);
        assert_eq!(
            (v["ok"].clone(), v["protocol"].clone()),
            (true.into(), 1.into()),
            "{v}"
        );
        v
    };
    let (c, out, e) = ppu(d, &["art", "check", "hero.png", "--json"], None);
    assert_eq!(c, 0, "{e}");
    proto(&out);
    // palette write: only hero.png names the palette and its PLTE changes
    let mut colours = String::new();
    for i in 0..32 {
        colours += &format!("{} 0 0\n", if i == 1 { 255 } else { 0 });
    }
    let (c, out, e) = ppu(
        d,
        &["art", "palette", "write", "pal.gpl", "--json"],
        Some(colours.as_bytes()),
    );
    assert_eq!(c, 0, "{e}");
    let v = proto(&out);
    let synced = v["synced"].as_array().unwrap();
    assert_eq!(synced.len(), 1, "{v}");
    assert!(synced[0].as_str().unwrap().ends_with("hero.png"), "{v}");
    // nothing left to sync: an empty list, not an absent field
    let (c, out, _) = ppu(d, &["art", "palette", "sync", "pal.gpl", "--json"], None);
    assert_eq!(c, 0);
    assert_eq!(proto(&out)["synced"], serde_json::json!([]));
    // rows
    let (c, out, e) = ppu(
        d,
        &[
            "art", "rows", "hero.png", "--width", "16", "--height", "16", "--fix", "--json",
        ],
        Some(&[0; 256]),
    );
    assert_eq!(c, 0, "{e}");
    let v = proto(&out);
    assert_eq!(
        (v["pixels"].as_str().unwrap().len(), v["notes"].is_array()),
        (512, true)
    );
    // new
    let (c, out, e) = ppu(
        d,
        &[
            "art",
            "new",
            "art/fresh.png",
            "--kind",
            "bg",
            "--size",
            "8x8",
            "--new-palette",
            "fresh_pal",
            "--json",
        ],
        None,
    );
    assert_eq!(c, 0, "{e}");
    assert_eq!(proto(&out)["files"].as_array().unwrap().len(), 3);
    // convert
    let rgba = rgba_png(
        16,
        16,
        |x, _| if x < 8 { [200, 30, 30] } else { [30, 30, 200] },
    );
    std::fs::write(d.join("art/photo.png"), rgba).unwrap();
    let (c, out, e) = ppu(
        d,
        &["art", "convert", "art/photo.png", "--kind", "bg", "--json"],
        None,
    );
    assert_eq!(c, 0, "{out}{e}");
    proto(&out);
    // install
    let u = tempfile::tempdir().unwrap();
    let us = u.path().to_str().unwrap();
    let (c, out, e) = ppu(
        d,
        &["art", "install", "aseprite", "--dir", us, "--json"],
        None,
    );
    assert_eq!(c, 0, "{e}");
    let v = proto(&out);
    assert_eq!(
        (
            v["files"].as_array().unwrap().len(),
            v["folder"].is_string(),
            v["after"].is_string()
        ),
        (2, true, true)
    );
}
