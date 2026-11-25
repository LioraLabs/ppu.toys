use ppu_core::tiled::{read_map, Binding, Map};
use ppu_core::{M7Options, M7Source, SheetSource};
use serde_json::{json, Value};

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/tiled/");

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!("{DIR}{name}")).unwrap()
}

/// A 4bpp sheet of a 32x32 px image: 16 chars, char c in palette c % 3.
fn sheet() -> (SheetSource, Vec<u8>) {
    let s = SheetSource {
        bit_depth: 4,
        palettes: vec![vec![0x7fff]; 3],
        char_words: vec![0; 16 * 16],
        pals: vec![],
    };
    (s, (0..16).map(|c| (c % 3) as u8).collect())
}

/// A 16x8 px Mode 7 image whose two cells deduplicated to tiles 5 and 7.
fn track(extbg: bool) -> M7Source {
    M7Source {
        options: M7Options {
            extbg,
            overflowed_tiles: 0,
        },
        palette: vec![0x7fff],
        tiles: vec![[0; 64]; 8],
        tiles_w: 2,
        tiles_h: 1,
        map: vec![5, 7],
    }
}

fn read(text: &str) -> Result<Map, String> {
    let (s, pals) = sheet();
    let m7 = track(true);
    read_map(
        text,
        |path| match path {
            "tiny.tsj" => Ok(fixture("tiny.tsj")),
            other => Err(format!("no file {other}")),
        },
        |name| match name {
            "tiles" => Some(Binding::Sheet {
                source: &s,
                pals: &pals,
            }),
            "track" => Some(Binding::M7(&m7)),
            _ => None,
        },
    )
}

/// The SNES BG word of sheet char `c` with the fixture's palette.
fn w(c: u16, h: bool, v: bool) -> Option<u16> {
    Some(c | (c % 3) << 10 | (h as u16) << 14 | (v as u16) << 15)
}

#[test]
fn sixteen_px_tiles_become_flipped_2x2_cell_groups_with_kinds() {
    let map = read(&fixture("level.tmj")).unwrap();
    assert_eq!((map.width, map.height, map.tile_size), (6, 4, 16));
    // Classes number by tile id, `class` and the older `type` alike.
    assert_eq!(map.kinds, ["ladder", "solid"]);
    assert_eq!(map.properties["music"], json!("cave"));

    let ground = &map.layers[0];
    assert_eq!(
        (ground.name.as_str(), ground.source.as_deref(), ground.mode7),
        ("ground", Some("tiles"), false)
    );
    assert_eq!(ground.properties["solid"], json!(true));
    let (f, t) = (false, true);
    #[rustfmt::skip]
    let cells = vec![
        w(0, f, f),  w(1, f, f),  w(3, t, f),  w(2, t, f),  None, None,
        w(4, f, f),  w(5, f, f),  w(7, t, f),  w(6, t, f),  None, None,
        w(12, f, t), w(13, f, t), w(10, f, f), w(11, f, f), None, None,
        w(8, f, t),  w(9, f, t),  w(14, f, f), w(15, f, f), None, None,
    ];
    assert_eq!(ground.cells, cells);
    #[rustfmt::skip]
    let kinds = vec![
        0, 0, 1, 1, 0, 0,
        0, 0, 1, 1, 0, 0,
        0, 0, 2, 2, 0, 0,
        0, 0, 2, 2, 0, 0,
    ];
    assert_eq!(ground.kinds, kinds);

    // CSV written as a string, as agents write it.
    let back = &map.layers[1];
    assert_eq!(back.cells.iter().filter(|c| c.is_some()).count(), 4);
    assert_eq!(back.cells[2 * 6 + 4], w(10, f, f));
    assert_eq!(back.kinds[3 * 6 + 5], 2);

    let things = &map.object_layers[0];
    assert_eq!(things.name, "things");
    let spawn = &things.objects[0];
    assert_eq!(
        (spawn.name.as_str(), spawn.kind.as_str()),
        ("spawn", "player")
    );
    assert_eq!((spawn.x, spawn.y, spawn.w, spawn.h), (16., 32., 16., 16.));
    assert_eq!(spawn.props["hp"], json!(3));
    let exit = &things.objects[1];
    assert_eq!(
        (exit.name.as_str(), exit.kind.as_str(), exit.w),
        ("exit", "door", 0.)
    );
}

#[test]
fn external_tsj_tileset_and_base64_data() {
    let map = read(&fixture("tiny.tmj")).unwrap();
    assert_eq!((map.width, map.height, map.tile_size), (2, 2, 8));
    assert_eq!(map.kinds, ["spike"]);
    let l = &map.layers[0];
    assert_eq!(
        l.cells,
        vec![
            w(0, false, false),
            w(5, false, false),
            None,
            w(15, true, false)
        ]
    );
    assert_eq!(l.kinds, vec![0, 1, 0, 0]);
}

#[test]
fn mode7_cells_are_the_m7_sources_tile_numbers() {
    let map = read(&fixture("track.tmj")).unwrap();
    let l = &map.layers[0];
    assert_eq!((l.source.as_deref(), l.mode7), (Some("track"), true));
    assert_eq!(l.cells, vec![Some(7), Some(5)]);
}

fn edit(name: &str, f: impl FnOnce(&mut Value)) -> String {
    let mut v: Value = serde_json::from_str(&fixture(name)).unwrap();
    f(&mut v);
    v.to_string()
}

fn refused(text: &str, says: &str) {
    let err = read(text).expect_err("expected a refusal");
    assert!(err.contains(says), "{err:?} does not say {says:?}");
}

#[test]
fn refuses_tmx() {
    refused("<?xml version=\"1.0\"?><map/>", "save it as JSON");
}

#[test]
fn refuses_compressed_layer_data() {
    for c in ["zlib", "gzip", "zstd"] {
        let text = edit("level.tmj", |v| {
            v["layers"][1]["encoding"] = json!("base64");
            v["layers"][1]["compression"] = json!(c);
            v["layers"][1]["data"] = json!("AAAA");
        });
        refused(&text, "set the layer format to CSV");
    }
}

#[test]
fn refuses_diagonal_flips() {
    let text = edit("level.tmj", |v| {
        v["layers"][0]["data"][0] = json!(1u32 | 0x2000_0000)
    });
    refused(&text, "no such flip");
}

#[test]
fn refuses_infinite_maps() {
    let text = edit("level.tmj", |v| v["infinite"] = json!(true));
    refused(&text, "untick Map > Infinite");
}

#[test]
fn refuses_isometric_hexagonal_and_staggered_maps() {
    for o in ["isometric", "hexagonal", "staggered"] {
        let text = edit("level.tmj", |v| v["orientation"] = json!(o));
        refused(&text, "set Map > Orientation to Orthogonal");
    }
}

#[test]
fn refuses_tile_sizes_other_than_8_and_16() {
    let text = edit("level.tmj", |v| {
        v["tilewidth"] = json!(32);
        v["tileheight"] = json!(32);
    });
    refused(&text, "8x8 or 16x16");
}

#[test]
fn refuses_a_tileset_image_with_no_source() {
    let text = edit("level.tmj", |v| {
        v["tilesets"][0]["image"] = json!("cave.png")
    });
    refused(&text, "add a source named cave");
}

#[test]
fn refuses_a_tileset_image_the_size_of_another_sheet() {
    let text = edit("level.tmj", |v| v["tilesets"][0]["imageheight"] = json!(16));
    refused(&text, "has 16 chars");
}

#[test]
fn refuses_rotated_objects() {
    let text = edit("level.tmj", |v| {
        v["layers"][2]["objects"][0]["rotation"] = json!(90)
    });
    refused(&text, "set its rotation to 0");
}

#[test]
fn refuses_flips_on_mode7() {
    let text = edit("track.tmj", |v| {
        v["layers"][0]["data"][0] = json!(2u32 | 0x8000_0000)
    });
    refused(&text, "Mode 7 has no tile flips");
}

#[test]
fn refuses_xml_tilesets_and_layers_mixing_tilesets() {
    let text = edit("tiny.tmj", |v| {
        v["tilesets"][0]["source"] = json!("tiny.tsx")
    });
    refused(&text, "export it as JSON");
    let text = edit("level.tmj", |v| {
        let mut second = v["tilesets"][0].clone();
        second["firstgid"] = json!(5);
        second["name"] = json!("more");
        v["tilesets"].as_array_mut().unwrap().push(second);
        v["layers"][0]["data"][2] = json!(5);
    });
    refused(&text, "a layer draws from one tileset");
}
