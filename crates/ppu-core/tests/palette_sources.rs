//! Palette sources: `.gpl` text becomes a payload kind that `dma` places by itself
//! (`dma("pal", { pal = n })`) or lends to art (`dma(art, { palette = "pal" })`).
mod common;

use ppu_core::gpl;
use ppu_core::{LuaEngine, PayloadError, SourcePayload};

const RED: u16 = 0x001f;
const BLUE: u16 = 0x7c00;
const GREEN: u16 = 0x03e0;

fn solid(w: usize, h: usize, rgb: [u8; 3]) -> Vec<u8> {
    (0..w * h)
        .flat_map(|_| [rgb[0], rgb[1], rgb[2], 255])
        .collect()
}

/// Add the `.gpl` made of `text` as palette source `name`.
fn add_palette(e: &mut LuaEngine, name: &str, columns: u32, text: &str) {
    let gpl = format!("GIMP Palette\nColumns: {columns}\n{text}");
    let (payload, _) = gpl::convert_palette(&gpl).unwrap();
    e.add_source(name, &payload.encode()).unwrap();
}

// Two 4-colour rows; each row's colour 0 is non-black, so a write to it would show.
const FOUR: &str = "255 255 255\n255 0 0\n0 0 255\n0 255 0\n255 255 255\n8 0 0\n0 0 8\n0 8 0\n";
const FIFTEEN: &str = "255 255 255\n255 0 0\n0 0 255\n0 255 0\n8 8 8\n16 16 16\n24 24 24\n32 32 32\n\
                       40 40 40\n48 48 48\n56 56 56\n64 64 64\n72 72 72\n80 80 80\n88 88 88\n96 96 96\n";

fn engine(setup: impl FnOnce(&mut LuaEngine), main: &str) -> Result<LuaEngine, String> {
    let mut e = LuaEngine::new();
    common::add_bg(&mut e, "art", solid(64, 64, [0, 255, 0]), 64, 64, 4);
    add_palette(&mut e, "pal", 16, FIFTEEN);
    setup(&mut e);
    e.set_sources(&[("main.luau", main)])
        .map(|_| e)
        .map_err(|e| format!("{e:?}"))
}

#[test]
fn payload_round_trips_and_decode_is_strict() {
    for (cols, text) in [(4, FOUR), (16, FIFTEEN)] {
        let p = gpl::parse(&format!("GIMP Palette\nColumns: {cols}\n{text}")).unwrap();
        let bytes = SourcePayload::Palette(p.clone()).encode();
        assert_eq!(&bytes[..4], [3, 6, cols as u8, p.rows.len() as u8]);
        assert_eq!(SourcePayload::decode(&bytes), Ok(SourcePayload::Palette(p)));
    }
    // The 256 block is stored at its real colour count (count - 1 in the row-count byte).
    let full: String = (0..256).map(|i| format!("{i} 0 0\n")).collect();
    for (text, n) in [("1 2 3\n4 5 6\n".to_string(), 1u8), (full, 255)] {
        let wide = gpl::parse(&format!("GIMP Palette\nColumns: 256\n{text}")).unwrap();
        let bytes = SourcePayload::Palette(wide.clone()).encode();
        assert_eq!(&bytes[..4], [3, 6, 0, n]);
        assert_eq!(bytes.len(), 4 + 2 * (n as usize + 1));
        assert_eq!(
            SourcePayload::decode(&bytes),
            Ok(SourcePayload::Palette(wide))
        );
    }

    let row = |n: usize| vec![0u8; n * 2];
    let with = |head: &[u8], body: Vec<u8>| [head, &body].concat();
    for (bytes, err) in [
        (
            with(&[3, 6, 8, 1], row(8)),
            PayloadError::BadParam("row_size"),
        ),
        (
            with(&[3, 6, 16, 0], vec![]),
            PayloadError::BadParam("row_count"),
        ),
        (
            with(&[3, 6, 16, 17], row(16 * 17)),
            PayloadError::BadParam("row_count"),
        ),
        (with(&[3, 6, 0, 2], row(2)), PayloadError::Truncated),
        (with(&[3, 6, 0, 0], row(2)), PayloadError::TrailingBytes),
        (with(&[3, 6, 4, 2], row(4)), PayloadError::Truncated),
        (with(&[3, 6, 4, 1], row(5)), PayloadError::TrailingBytes),
    ] {
        assert_eq!(SourcePayload::decode(&bytes), Err(err));
    }
}

#[test]
fn dma_of_a_palette_writes_its_rows_but_never_entry_0() {
    let mut e = engine(
        |e| add_palette(e, "p4", 4, FOUR),
        r#"
local p = dma("p4", { pal = 8 })
assert(p.pal == 8 and p.next_pal == 16, "result")
function frame(t, f) end
"#,
    )
    .unwrap();
    e.frame(0.0, 0).unwrap();
    assert_eq!(
        e.memory().cgram[8..16],
        [0, RED, BLUE, GREEN, 0, 1, 1 << 10, 1 << 5]
    );
}

#[test]
fn art_takes_the_named_palettes_colours_and_keeps_its_own_tiles() {
    let main = |opts: &str| format!("dma(\"art\", {opts})\nfunction frame(t, f) end");
    let mut plain = engine(|_| {}, &main("{}")).unwrap();
    let mut lent = engine(|_| {}, &main("{ palette = \"pal\" }")).unwrap();
    plain.frame(0.0, 0).unwrap();
    lent.frame(0.0, 0).unwrap();
    assert_eq!(plain.memory().cgram[1], GREEN, "embedded colour");
    assert_eq!(lent.memory().cgram[1..4], [RED, BLUE, GREEN]);
    assert_eq!(lent.memory().cgram[0], 0, "entry 0 is not written");
    assert_eq!(
        plain.memory().vram[..],
        lent.memory().vram[..],
        "same tiles and map"
    );
}

#[test]
fn every_palette_row_lands_at_the_arts_palette_slots() {
    let two = format!("{FIFTEEN}{FIFTEEN}").replace("255 0 0", "0 0 255");
    let mut e = engine(
        |e| add_palette(e, "two", 16, &two),
        r#"
local g = dma("art", { palette = "two", pal = 16 })
assert(g.pal == 16 and g.next_pal == 48, "next_pal counts the palette's rows")
function frame(t, f) end
"#,
    )
    .unwrap();
    e.frame(0.0, 0).unwrap();
    let cg = &e.memory().cgram;
    assert_eq!((cg[16], cg[17], cg[32], cg[33]), (0, BLUE, 0, BLUE));
}

#[test]
fn obj_art_takes_the_palette_at_its_obj_palette_number() {
    let mut e = engine(
        |e| common::add_obj(e, "ship", solid(8, 8, [0, 255, 0]), 8, 8),
        r#"
dma("ship", { char = 0x2000, pal = 1, palette = "pal" })
function frame(t, f) end
"#,
    )
    .unwrap();
    e.frame(0.0, 0).unwrap();
    assert_eq!(e.memory().cgram[128 + 16..128 + 19], [0, RED, BLUE]);
}

#[test]
fn m7_art_takes_a_256_block() {
    let text: String = (0..256).map(|i| format!("{} 0 0\n", i)).collect();
    let mut e = engine(
        |e| {
            common::add_m7(e, "floor", solid(16, 16, [0, 255, 0]), 16, 16);
            add_palette(e, "block", 256, &text);
        },
        r#"
dma("floor", { palette = "block" })
function frame(t, f) end
"#,
    )
    .unwrap();
    e.frame(0.0, 0).unwrap();
    let cg = &e.memory().cgram;
    assert_eq!((cg[0], cg[1], cg[255]), (0, 1 >> 3, 255 >> 3));
}

#[test]
fn an_m7_palette_claims_only_its_real_colours_so_sprites_still_fit() {
    let mut e = engine(
        |e| {
            common::add_m7(e, "road", solid(16, 16, [0, 255, 0]), 16, 16);
            common::add_obj(e, "ship", solid(8, 8, [0, 0, 255]), 8, 8);
            add_palette(e, "road_pal", 256, "255 255 255\n255 0 0\n0 0 255\n");
        },
        r#"
dma("road", { palette = "road_pal" })
dma("ship", { char = 0x6000 })
function frame(t, f) end
"#,
    )
    .unwrap();
    e.frame(0.0, 0).unwrap();
    let cg = &e.memory().cgram;
    assert_eq!(cg[0..4], [0, RED, BLUE, 0], "only entries 1..3 are written");
    assert_eq!(cg[129], BLUE, "the ship's own palette survives the road's");
}

#[test]
fn an_m7_palette_with_fewer_colours_than_the_art_uses_is_refused() {
    let err = engine(
        |e| {
            common::add_m7(e, "road", solid(16, 16, [0, 255, 0]), 16, 16);
            add_palette(e, "road_pal", 256, "255 255 255\n");
        },
        "dma(\"road\", { palette = \"road_pal\" })\nfunction frame(t, f) end",
    )
    .err()
    .expect("entry 0 alone cannot colour the art");
    assert!(
        err.contains(
            "dma: 'road' with palette 'road_pal': the art uses 2 colours but the palette has 1"
        ),
        "{err}"
    );
}

#[test]
fn a_256_palette_placed_alone_claims_only_its_real_colours() {
    let mut e = engine(
        |e| {
            common::add_obj(e, "ship", solid(8, 8, [0, 0, 255]), 8, 8);
            add_palette(e, "road_pal", 256, "255 255 255\n255 0 0\n0 0 255\n");
        },
        r#"
local p = dma("road_pal")
assert(p.next_pal == 3, "next_pal is the block's colour count")
dma("ship", { char = 0x6000 })
function frame(t, f) end
"#,
    )
    .unwrap();
    e.frame(0.0, 0).unwrap();
    assert_eq!(e.memory().cgram[0..4], [0, RED, BLUE, 0]);
}

#[test]
fn an_8bpp_bg_palette_with_fewer_colours_than_the_art_uses_is_refused() {
    let err = engine(
        |e| {
            common::add_bg(e, "wide", solid(8, 8, [0, 255, 0]), 8, 8, 8);
            add_palette(e, "short", 256, "255 255 255\n");
        },
        "dma(\"wide\", { palette = \"short\" })\nfunction frame(t, f) end",
    )
    .err()
    .expect("entry 0 alone cannot colour the art");
    assert!(
        err.contains(
            "dma: 'wide' with palette 'short': the art uses 2 colours but the palette has 1"
        ),
        "{err}"
    );
}

#[test]
fn a_stage_load_takes_the_palette_too() {
    let mut e = engine(
        |_| {},
        r#"
function frame(t, f)
  force_blank = true
  dma("art", { palette = "pal", pal = 16 })
end
"#,
    )
    .unwrap();
    e.frame(0.0, 0).unwrap();
    assert_eq!(e.memory().cgram[17..19], [RED, BLUE]);
    assert_eq!(e.loads()[0].cgram, (16, 32));
}

#[test]
fn replacing_the_palette_source_changes_the_next_frame() {
    let mut e = engine(
        |_| {},
        "dma(\"art\", { palette = \"pal\" })\nfunction frame(t, f) end",
    )
    .unwrap();
    e.frame(0.0, 0).unwrap();
    assert_eq!(e.memory().cgram[1], RED);
    add_palette(&mut e, "pal", 16, &FIFTEEN.replace("255 0 0", "0 0 255"));
    e.frame(1.0 / 60.0, 1).unwrap();
    assert_eq!(e.memory().cgram[1], BLUE);
}

#[test]
fn refusals_name_the_problem() {
    for (call, want) in [
        (
            "dma(\"art\", { palette = \"nope\" })",
            "dma: no palette source named 'nope'",
        ),
        (
            "dma(\"art\", { palette = \"art\" })",
            "dma: 'art' is a bg source, not a palette",
        ),
        (
            "dma(\"art\", { palette = 3 })",
            "dma: opts.palette must be a palette source name",
        ),
        (
            "dma(\"art\", { palette = \"p4\" })",
            "dma: 'art' with palette 'p4': the art takes 16-colour palette rows but the palette has 4-colour rows",
        ),
        (
            "dma(\"pal\", { char = 0x1000 })",
            "'pal' is a palette source — it takes only 'pal', not 'char'",
        ),
        (
            "dma(\"pal\", { palette = \"pal\" })",
            "it takes only 'pal', not 'palette'",
        ),
        (
            "dma(\"pal\", { pal = 300 })",
            "dma: opts.pal must be an integer in 0..0xff",
        ),
        (
            "dma(\"pal\", { pal = 250 })",
            "dma: 'pal' placement exceeds PPU memory",
        ),
    ] {
        let err = engine(
            |e| add_palette(e, "p4", 4, FOUR),
            &format!("{call}\nfunction frame(t, f) end"),
        )
        .err()
        .unwrap_or_else(|| panic!("{call} should be refused"));
        assert!(err.contains(want), "{call}: {err}");
    }
}

#[test]
fn a_refused_dma_records_nothing() {
    let e = engine(
        |_| {},
        r#"
assert(not pcall(dma, "art", { palette = "nope" }))
assert(not pcall(dma, "pal", { pal = 250 }))
function frame(t, f) end
"#,
    )
    .unwrap();
    let map = e.memory_map();
    assert!(map.placements.is_empty() && map.cgram.is_empty() && map.vram.is_empty());
}

#[test]
fn a_dma_call_is_one_placement() {
    let e = engine(
        |_| {},
        "dma(\"art\", { palette = \"pal\" })\nfunction frame(t, f) end",
    )
    .unwrap();
    let map = e.memory_map();
    assert_eq!((map.placements.len(), map.cgram.len()), (1, 1));
    assert_eq!((map.cgram[0].start, map.cgram[0].end), (0, 16));
}

/// Two 8x8 tiles of 15 distinct colours each: too many for one 4bpp row, so two sub-palettes.
fn two_row_art(e: &mut LuaEngine) {
    let mut rgba = Vec::new();
    for y in 0..8usize {
        for x in 0..16usize {
            let c = ((y * 8 + x % 8) % 15) as u8 * 8;
            let rgb = if x < 8 {
                [c, 255 - c, 100]
            } else {
                [100, c, 255 - c]
            };
            rgba.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
        }
    }
    common::add_bg(e, "two_art", rgba, 16, 8, 4);
}

#[test]
fn a_palette_with_fewer_rows_than_the_art_uses_is_refused() {
    let err = engine(
        |e| two_row_art(e),
        "dma(\"two_art\", { palette = \"pal\" })\nfunction frame(t, f) end",
    )
    .err()
    .expect("one row cannot cover two");
    assert!(
        err.contains(
            "dma: 'two_art' with palette 'pal': the art uses 2 palette rows but the palette has 1"
        ),
        "{err}"
    );
}

/// An obj sheet of two 8x8 cells with 15 distinct colours each: two palette rows.
fn two_row_obj(e: &mut LuaEngine) {
    let mut rgba = Vec::new();
    for y in 0..8usize {
        for x in 0..16usize {
            let c = ((y * 8 + x % 8) % 15) as u8 * 8;
            let rgb = if x < 8 { [c, 255 - c, 100] } else { [100, c, 255 - c] };
            rgba.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
        }
    }
    common::add_obj(e, "two", rgba, 16, 8);
}

const VARIANTS: &str = r#"
local ship = dma("ship", { char = 0x2000, pal = 1 })
local base = sprites.art({{size = 8, columns = 1, cells = {0}}}, ship)
local red = sprites.art({{size = 8, columns = 1, cells = {0}, palette = "pal", pal = 4}}, ship)
local a, b = sprites.group(), sprites.group()
function frame(t, f)
  a:draw(base, 40, 40)
  b:draw(red, 80, 40)
end
"#;

fn ship_engine(main: &str) -> Result<LuaEngine, String> {
    engine(
        |e| common::add_obj(e, "ship", solid(8, 8, [0, 255, 0]), 8, 8),
        main,
    )
}

#[test]
fn a_sprite_level_takes_a_palette_and_two_groups_show_both_colourings() {
    let mut e = ship_engine(VARIANTS).unwrap();
    e.frame(0.0, 0).unwrap();
    let m = e.memory();
    assert_eq!(m.cgram[128 + 64 + 1..128 + 64 + 4], [RED, BLUE, GREEN]);
    assert_eq!(m.cgram[128 + 64], 0, "entry 0 is not written");
    let pals: Vec<u8> = m.oam.iter().filter(|o| o.on).map(|o| o.pal).collect();
    assert_eq!(
        pals,
        [1, 4],
        "base keeps the sheet's palette, the variant its own"
    );
    let map = e.memory_map();
    assert!(
        map.cgram.iter().any(|r| r.start == 128 + 64),
        "the variant is placed like a file-level dma: {:?}",
        map.cgram
    );
}

#[test]
fn a_level_without_palette_is_unchanged() {
    let mut e = ship_engine(&VARIANTS.replace(r#", palette = "pal", pal = 4"#, "")).unwrap();
    e.frame(0.0, 0).unwrap();
    let pals: Vec<u8> = e
        .memory()
        .oam
        .iter()
        .filter(|o| o.on)
        .map(|o| o.pal)
        .collect();
    assert_eq!(pals, [1, 1]);
}

#[test]
fn sprite_art_palette_refusals_name_the_problem() {
    for (level, want) in [
        (
            r#"palette = 3, pal = 4"#,
            "sprites.art level palette must be a palette source name",
        ),
        (r#"palette = "pal""#, "sprites.art level palette needs pal"),
        (
            r#"palette = "nope", pal = 4"#,
            "sprites.art level palette nope: no palette source named 'nope'",
        ),
        (
            r#"palette = "p4", pal = 4"#,
            "sprites.art level palette p4: 'ship' with palette 'p4': the art takes 16-colour palette rows but the palette has 4-colour rows",
        ),
        (
            r#"palette = "bgart", pal = 4"#,
            "sprites.art level palette bgart: 'bgart' is a bg source, not a palette",
        ),
        (
            r#"palette = "ship", pal = 4"#,
            "sprites.art level palette ship: 'ship' is a obj source, not a palette",
        ),
    ] {
        let main = format!(
            "local ship = dma(\"ship\", {{ char = 0x2000, pal = 1 }})\n\
             sprites.art({{{{size = 8, columns = 1, cells = {{0}}, {level}}}}}, ship)\nfunction frame(t, f) end"
        );
        let err = engine(
            |e| {
                common::add_obj(e, "ship", solid(8, 8, [0, 255, 0]), 8, 8);
                add_palette(e, "p4", 4, FOUR);
                common::add_bg(e, "bgart", solid(8, 8, [255, 0, 0]), 8, 8, 4);
            },
            &main,
        )
        .err()
        .unwrap_or_else(|| panic!("{level} should be refused"));
        assert!(err.contains(want), "{level}: {err}");
    }
}

#[test]
fn a_variant_past_obj_palette_7_is_refused() {
    let main = r#"
local ship = dma("ship", { char = 0x2000, pal = 1 })
sprites.art({{size = 8, columns = 1, cells = {0}, palette = "wide", pal = 7}}, ship)
function frame(t, f) end
"#;
    let err = engine(
        |e| {
            common::add_obj(e, "ship", solid(8, 8, [0, 255, 0]), 8, 8);
            add_palette(e, "wide", 16, &format!("{FIFTEEN}{FIFTEEN}"));
        },
        main,
    )
    .err()
    .expect("refused");
    assert!(
        err.contains("sprites.art level palette wide has 2 rows and goes to OBJ palettes 7..8, past palette 7"),
        "{err}"
    );
}

#[test]
fn a_palette_with_fewer_rows_than_the_sheet_uses_is_refused_for_a_level() {
    let main = r#"
local ship = dma("two", { char = 0x2000, pal = 1 })
sprites.art({{size = 8, columns = 1, cells = {0}, palette = "pal", pal = 4}}, ship)
function frame(t, f) end
"#;
    let err = engine(two_row_obj, main).err().expect("refused");
    assert!(
        err.contains("sprites.art level palette pal: 'two' with palette 'pal': the art uses 2 palette rows but the palette has 1"),
        "{err}"
    );
}

#[test]
fn a_refused_level_places_nothing() {
    let e = ship_engine(
        r#"
local ship = {tile = 0, pal = 0, cells = {{tile = 0}}}
assert(not pcall(sprites.art, {{size = 8, columns = 1, cells = {0}, palette = "pal"}}, ship))
assert(not pcall(sprites.art, {{size = 8, columns = 1, cells = {0}, palette = "pal", pal = 8}}, ship))
function frame(t, f) end
"#,
    )
    .unwrap();
    let map = e.memory_map();
    assert!(map.placements.is_empty() && map.cgram.is_empty());
}
