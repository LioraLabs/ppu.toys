//! @ppu/world in the preview: a Tiled map larger than the screen streams into a background as
//! the camera moves, and every frame matches the whole map drawn straight from the sheet image.
mod common;

use common::{add_sheet, engine_with};
use ppu_core::{render_frame, LuaEngine, HEIGHT, WIDTH};
use serde_json::json;

const MAP_W: usize = 256;
const MAP_H: usize = 64;
const FLIP_H: u32 = 0x8000_0000;
const FLIP_V: u32 = 0x4000_0000;

/// A 64x16 px sheet of 16 chars: char 0 blank, chars 1..7 in one set of colours and 8..15 in
/// another, so the importer needs two palettes; each char's pattern is its own and asymmetric,
/// so a wrong char or a missed flip shows.
fn sheet() -> Vec<u8> {
    // Eight colours a set, sixteen in all: more than one 15-colour palette holds.
    let color = |set: usize, k: usize| -> [u8; 3] {
        let (a, b) = ((248 - k * 16) as u8, (k * 24 + 8) as u8);
        if set == 0 {
            [a, b, 64]
        } else {
            [64, a, b]
        }
    };
    let mut rgba = vec![0u8; 64 * 16 * 4];
    for c in 1..16usize {
        for py in 0..8 {
            for px in 0..8 {
                let v = (px * 3 + py * 5 + c * 7 + px * py * c) % 5;
                if v == 0 {
                    continue;
                }
                let at = (((c / 8) * 8 + py) * 64 + (c % 8) * 8 + px) * 4;
                let rgb = color(c / 8, (v + px / 4 + c) % 8);
                rgba[at..at + 4].copy_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
            }
        }
    }
    rgba
}

/// The map's gids: pseudo-random chars with flips, some empty.
fn gids() -> Vec<u32> {
    let mut s: u32 = 0x1234_5678;
    (0..MAP_W * MAP_H)
        .map(|_| {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            match s % 20 {
                0 => 0,
                n => {
                    1 + (n % 15)
                        + if s & 0x100 != 0 { FLIP_H } else { 0 }
                        + if s & 0x200 != 0 { FLIP_V } else { 0 }
                }
            }
        })
        .collect()
}

fn tmj(width: usize, height: usize, data: &[u32]) -> String {
    json!({
        "orientation": "orthogonal", "infinite": false, "tilewidth": 8, "tileheight": 8,
        "width": width, "height": height,
        "tilesets": [{
            "firstgid": 1, "name": "cave", "image": "art/tiles.png", "imagewidth": 64, "imageheight": 16,
            "tilewidth": 8, "tileheight": 8, "columns": 8, "tilecount": 16,
            "tiles": [{"id": 3, "type": "solid"}, {"id": 9, "type": "spike"}, {"id": 4, "type": "solid"}]
        }],
        "layers": [
            {"type": "tilelayer", "name": "ground", "width": width, "height": height, "data": data},
            {"type": "objectgroup", "name": "things", "objects": [
                {"name": "start", "type": "spawn", "x": 40, "y": 96, "width": 0, "height": 0,
                 "properties": [{"name": "facing", "type": "string", "value": "left"}, {"name": "hp", "type": "int", "value": 3}]},
                {"name": "door", "type": "exit", "x": 1000.5, "y": 8, "width": 16, "height": 32}
            ]}
        ]
    })
    .to_string()
}

/// Camera path segments (dx, dy per frame, frames): diagonals and reversals under 8 px a frame that
/// cross x = 512, 1024, 1536 and y = 256 several times.
const PATH: &[(i64, i64, i64)] = &[
    (4, 1, 200),
    (4, -2, 100),
    (-3, 2, 144),
    (4, 0, 256),
    (-4, -4, 72),
    (-4, 3, 96),
    (-2, -3, 96),
];

fn camera(f: i64) -> (i64, i64) {
    let (mut x, mut y, mut left) = (0, 0, f);
    for &(dx, dy, n) in PATH {
        let k = left.min(n);
        x += dx * k;
        y += dy * k;
        left -= k;
    }
    (x, y)
}

fn main_luau() -> String {
    let segs: Vec<String> = PATH
        .iter()
        .map(|(dx, dy, n)| format!("{{{dx}, {dy}, {n}}}"))
        .collect();
    format!(
        r#"local world = require("@ppu/world")
local tiles = dma("tiles", {{char = 0x2000, pal = 32}})
local w = world.load("level1")
local PATH = {{{}}}
local x, y, seg, left = 0, 0, 1, PATH[1][3]

function init()
  mode = 1
  brightness = 15
  bg[1].map_base = 0x0800
  w:bind("ground", bg[1])
  w:follow(0, 0)
  screen.main.bg1 = true
end

function frame(t, f)
  if f > 0 and seg <= #PATH then
    x, y = x + PATH[seg][1], y + PATH[seg][2]
    left -= 1
    if left == 0 then
      seg += 1
      if seg <= #PATH then left = PATH[seg][3] end
    end
  end
  w:follow(x, y)
end
"#,
        segs.join(", ")
    )
}

fn engine(main: &str, map: &str) -> Result<LuaEngine, String> {
    let mut e = LuaEngine::new();
    add_sheet(&mut e, "tiles", sheet(), 64, 16, 4);
    e.set_sources(&[("main.luau", main), ("level1.tmj", map)])
        .map_err(|e| e.message)?;
    Ok(e)
}

/// The colour at map pixel (x, y), read from the sheet image through the gids; black where nothing is drawn.
fn reference(sheet: &[u8], gids: &[u32], x: usize, y: usize) -> [u8; 3] {
    reference_w(sheet, gids, MAP_W, x, y)
}

fn reference_w(sheet: &[u8], gids: &[u32], width: usize, x: usize, y: usize) -> [u8; 3] {
    let raw = gids[(y / 8) * width + x / 8];
    let id = (raw & 0x0fff_ffff) as usize;
    if id == 0 {
        return [0; 3];
    }
    let c = id - 1;
    let (mut px, mut py) = (x % 8, y % 8);
    if raw & FLIP_H != 0 {
        px = 7 - px;
    }
    if raw & FLIP_V != 0 {
        py = 7 - py;
    }
    let at = (((c / 8) * 8 + py) * 64 + (c % 8) * 8 + px) * 4;
    if sheet[at + 3] == 0 {
        [0; 3]
    } else {
        [sheet[at] >> 3, sheet[at + 1] >> 3, sheet[at + 2] >> 3]
    }
}

#[test]
fn a_streamed_layer_matches_the_whole_map_at_every_sampled_camera() {
    let (sheet, gids) = (sheet(), gids());
    let mut e = engine(&main_luau(), &tmj(MAP_W, MAP_H, &gids)).unwrap();
    let total: i64 = PATH.iter().map(|p| p.2).sum();
    let mut checked = 0;
    for f in 0..=total {
        let lt = e
            .frame(f as f64 / 60.0, f as u32)
            .unwrap_or_else(|err| panic!("frame {f}: {err:?}"));
        if f % 23 != 0 && f != total {
            continue;
        }
        let (cx, cy) = camera(f);
        let fb = render_frame(&lt, e.memory());
        for sy in 0..HEIGHT {
            for sx in 0..WIDTH {
                let p = &fb[(sy * WIDTH + sx) * 4..][..3];
                let got = [p[0] >> 3, p[1] >> 3, p[2] >> 3];
                let want = reference(&sheet, &gids, cx as usize + sx, cy as usize + sy);
                assert_eq!(
                    got, want,
                    "frame {f}, camera {cx},{cy}: screen pixel {sx},{sy}"
                );
            }
        }
        checked += 1;
    }
    assert!(checked > 30, "only {checked} samples");
    assert_eq!(camera(total), (928, 0));
}

/// A layer bound with a parallax factor of n/16 shows the map at the camera's (x * n // 16, y * n // 16) every
/// sampled frame: its strips and scroll follow its own camera, across its own screen edges.
#[test]
fn a_parallax_layer_matches_the_whole_map_at_its_own_camera() {
    let (sheet, gids) = (sheet(), gids());
    let total: i64 = PATH.iter().map(|p| p.2).sum();
    for (factor, n) in [("0.5", 8), ("0.75", 12), ("0.0625", 1)] {
        let main = main_luau().replace(
            "w:bind(\"ground\", bg[1])",
            &format!("w:bind(\"ground\", bg[1], {{parallax = {factor}}})"),
        );
        let mut e = engine(&main, &tmj(MAP_W, MAP_H, &gids)).unwrap();
        for f in 0..=total {
            let lt = e
                .frame(f as f64 / 60.0, f as u32)
                .unwrap_or_else(|err| panic!("parallax {factor}, frame {f}: {err:?}"));
            if f % 17 != 0 && f != total {
                continue;
            }
            let (cx, cy) = camera(f);
            let (lx, ly) = (
                (cx * n).div_euclid(16) as usize,
                (cy * n).div_euclid(16) as usize,
            );
            let fb = render_frame(&lt, e.memory());
            for sy in 0..HEIGHT {
                for sx in 0..WIDTH {
                    let p = &fb[(sy * WIDTH + sx) * 4..][..3];
                    let want = reference(&sheet, &gids, lx + sx, ly + sy);
                    assert_eq!([p[0] >> 3, p[1] >> 3, p[2] >> 3], want, "parallax {factor}, frame {f}, layer camera {lx},{ly}: screen pixel {sx},{sy}");
                }
            }
        }
    }
}

#[test]
fn a_parallax_factor_is_sixteenths_from_one_sixteenth_to_one() {
    for bad in ["1.5", "0.3", "0", "-0.5", "\"half\""] {
        fails(
            &format!(
                "{}function frame() end",
                BOUND.replace("bg[1])", &format!("bg[1], {{parallax = {bad}}})"))
            ),
            "parallax is the layer's scroll rate as a multiple of 1/16",
        );
    }
    fails(
        &format!(
            "{}function frame() end",
            BOUND.replace("bg[1])", "bg[1], {speed = 2})")
        ),
        "the options are parallax, wrap, map and priority (got speed)",
    );
    fails(
        &format!(
            "{}function frame() end",
            BOUND.replace("bg[1])", "bg[1], 0.5)")
        ),
        "the third argument is the options table",
    );
    // The factor 1 is the camera's rate.
    engine(
        &format!(
            "{}function frame(t, f) w:follow(f * 4, 0) end",
            BOUND.replace("bg[1])", "bg[1], {parallax = 1})")
        ),
        &tmj(64, 32, &vec![1u32; 64 * 32]),
    )
    .unwrap();
}

/// `{map = m}` places the background's map at m (with or without parallax); a map off the 0x400 grid, or one another
/// background of the same world uses, is refused, by the option or by map_base set before the bind.
#[test]
fn the_map_option_places_the_map_and_one_world_never_shares_a_map() {
    let main = format!(
        "{}function frame() end",
        BOUND.replace("bg[1])", "bg[1], {map = 0x0400, parallax = 0.5})")
    );
    let mut e = engine(&main, &tmj(32, 32, &vec![2u32; 32 * 32])).unwrap();
    assert_eq!(e.frame(0.0, 0).unwrap().rows[100].bg[0].map_base, 0x400);
    let vram = &e.memory().vram;
    assert!(
        vram[0x400..0x800].iter().any(|&w| w != 0) && vram[..0x400].iter().all(|&w| w == 0),
        "the map should be at 0x0400"
    );
    for bad in ["0x0200", "0x8000", "\"low\""] {
        fails(
            &format!(
                "{}function frame() end",
                BOUND.replace("bg[1])", &format!("bg[1], {{map = {bad}}})"))
            ),
            "map is the background map's VRAM word address",
        );
    }
    for second in [
        "w:bind(\"ground\", bg[2], {map = 0x0000})",
        "bg[2].map_base = 0x0000 w:bind(\"ground\", bg[2])",
    ] {
        let main = BOUND.replace(
            "bg[1])\n",
            &format!("bg[1], {{map = 0x0000}})\n  {second}\n"),
        );
        fails(
            &format!("{main}function frame() end"),
            "bg[2]'s map at 0x0000 overlaps bg[1]'s at 0x0000",
        );
    }
}

/// A layer bound with wrap repeats its painted part (up to its last column and row with a tile) every sampled frame,
/// at its own parallax camera: the map's cells past that part are empty, yet the screen shows the repeat.
#[test]
fn a_wrapping_layer_repeats_its_painted_part() {
    let (sheet, gids) = (sheet(), gids());
    let (pw, ph) = (40, 20);
    let mut painted = vec![0u32; MAP_W * MAP_H];
    for y in 0..ph {
        for x in 0..pw {
            painted[y * MAP_W + x] = gids[y * MAP_W + x];
        }
    }
    // The part's last column and row hold a tile, so it is exactly pw x ph.
    painted[(ph - 1) * MAP_W + pw - 1] = 1;
    let repeated: Vec<u32> = (0..MAP_W * MAP_H)
        .map(|i| painted[(i / MAP_W % ph) * MAP_W + i % MAP_W % pw])
        .collect();
    let main = main_luau().replace(
        "w:bind(\"ground\", bg[1])",
        "w:bind(\"ground\", bg[1], {parallax = 0.75, wrap = true})",
    );
    let mut e = engine(&main, &tmj(MAP_W, MAP_H, &painted)).unwrap();
    let total: i64 = PATH.iter().map(|p| p.2).sum();
    for f in 0..=total {
        let lt = e
            .frame(f as f64 / 60.0, f as u32)
            .unwrap_or_else(|err| panic!("frame {f}: {err:?}"));
        if f % 17 != 0 && f != total {
            continue;
        }
        let (cx, cy) = camera(f);
        let (lx, ly) = (
            (cx * 12).div_euclid(16) as usize,
            (cy * 12).div_euclid(16) as usize,
        );
        let fb = render_frame(&lt, e.memory());
        for sy in 0..HEIGHT {
            for sx in 0..WIDTH {
                let p = &fb[(sy * WIDTH + sx) * 4..][..3];
                assert_eq!(
                    [p[0] >> 3, p[1] >> 3, p[2] >> 3],
                    reference(&sheet, &repeated, lx + sx, ly + sy),
                    "frame {f}, layer camera {lx},{ly}: screen pixel {sx},{sy}"
                );
            }
        }
    }
}

#[test]
fn wrap_is_true_or_false_and_needs_a_painted_layer() {
    fails(
        &format!(
            "{}function frame() end",
            BOUND.replace("bg[1])", "bg[1], {wrap = 1})")
        ),
        "wrap is true or false (got 1)",
    );
    let main = format!(
        "{}function frame() end",
        BOUND.replace("bg[1])", "bg[1], {wrap = true})")
    );
    let err = engine(&main, &tmj(64, 32, &vec![0u32; 64 * 32]))
        .err()
        .unwrap_or_default();
    assert!(err.contains("layer ground has no tiles"), "{err}");
}

#[test]
fn kinds_objects_and_tile_lookup_come_from_the_map() {
    let mut data = vec![1u32; 64 * 32];
    data[2 * 64 + 5] = 4; // tile id 3: solid
    data[2 * 64 + 6] = 10; // tile id 9: spike
    let main = r#"local world = require("@ppu/world")
local w = world.load("level1")
local o = w.objects
assert(w.kinds.solid == 1 and w.kinds.spike == 2, "kinds")
assert(w.width == 64 and w.height == 32, "size")
assert(w:tile("ground", 5 * 8, 2 * 8 + 7) == w.kinds.solid, "solid at a pixel inside the cell")
assert(w:tile("ground", 6 * 8 + 3, 2 * 8) == w.kinds.spike, "spike")
assert(w:tile("ground", 0, 0) == 0, "an unclassed tile is 0")
assert(w:tile("ground", -1, 0) == 0 and w:tile("ground", 0, 9999) == 0, "off the map is 0")
assert(#o == 2 and o[1].name == "start" and o[1].type == "spawn" and o[1].x == 40 and o[1].y == 96, "spawn")
assert(o[1].props.facing == "left" and o[1].props.hp == 3, "properties")
assert(o[2].type == "exit" and o[2].x == 1000.5 and o[2].w == 16 and o[2].h == 32, "door")
assert(w:find("start") == o[1] and w:find("door") == o[2], "find by name")
function frame() end
"#;
    engine(main, &tmj(64, 32, &data)).unwrap();
}

#[test]
fn find_refuses_a_name_the_map_lacks_or_shares() {
    fails(
        "local world = require(\"@ppu/world\")\nlocal w = world.load(\"level1\")\nlocal o = w:find(\"boss\")\nfunction frame() end\n",
        "w:find(\"boss\"): map level1 has no object named boss",
    );
    let map = tmj(64, 32, &vec![1u32; 64 * 32]).replace("\"door\"", "\"start\"");
    let main = "local world = require(\"@ppu/world\")\nlocal w = world.load(\"level1\")\nlocal o = w:find(\"start\")\nfunction frame() end\n";
    let err = engine(main, &map).err().expect("a shared name is refused");
    assert!(
        err.contains("map level1 has two or more objects named start"),
        "{err}"
    );
}

fn fails(main: &str, needle: &str) {
    let map = tmj(64, 32, &vec![1u32; 64 * 32]);
    let err = match engine(main, &map) {
        Err(m) => m,
        Ok(mut e) => match (0..3).try_for_each(|f| e.frame(f as f64 / 60.0, f).map(|_| ())) {
            Err(err) => err.message,
            Ok(()) => panic!("should have failed with {needle:?}"),
        },
    };
    assert!(err.contains(needle), "{needle:?} not in {err}");
}

const BOUND: &str = r#"local world = require("@ppu/world")
local tiles = dma("tiles", {char = 0x1000, pal = 0})
local w = world.load("level1")
function init()
  w:bind("ground", bg[1])
  w:follow(0, 0)
end
"#;

#[test]
fn speed_without_a_view_is_ignored_and_the_camera_stays_on_the_map() {
    fails(
        &format!("{BOUND}function frame(t, f) w:follow(-1, 0) end"),
        "leaves the map",
    );
    // Toys from before views wrote a speed (any number): it does nothing, and the toy runs.
    for speed in ["4", "12"] {
        let main = BOUND.replace(
            "world.load(\"level1\")",
            &format!("world.load(\"level1\", {{speed = {speed}}})"),
        );
        let mut e = engine(&main, &tmj(64, 32, &vec![1u32; 64 * 32])).unwrap();
        (0..3).for_each(|f| {
            e.frame(f as f64 / 60.0, f).unwrap();
        });
    }
    fails(
        &format!("{BOUND}function frame(t, f) w:follow(0, 0, {{cut = 1}}) end"),
        "cut is true or false",
    );
    fails(
        &format!("{BOUND}function frame(t, f) w:follow(0, 0, {{teleport = true}}) end"),
        "the one option is cut",
    );
}

/// Camera stops reached in one frame each: moves of 8, 16 and 40 px on each axis and diagonally, reversals, jumps
/// past the whole window both ways, and a teleport with {cut = true} to where the camera already is.
const JUMPS: &[(i64, i64)] = &[
    (0, 0),
    (8, 0),
    (24, 0),
    (64, 0),
    (64, 8),
    (64, 24),
    (64, 64),
    (104, 104),
    (64, 64),
    (48, 48),
    (8, 8),
    (900, 200),
    (1500, 260),
    (300, 0),
    (296, 3),
    (1200, 288),
    (0, 288),
    (0, 0),
    (40, 16),
    (40, 16),
];

#[test]
fn a_follow_streams_every_row_and_column_a_move_of_any_length_uncovers() {
    let (sheet, gids) = (sheet(), gids());
    let stops: Vec<String> = JUMPS.iter().map(|(x, y)| format!("{{{x}, {y}}}")).collect();
    // main_luau's setup, then a frame() that jumps between the stops.
    let setup = main_luau();
    let main = format!(
        "{}local STOPS = {{{}}}\nfunction frame(t, f)\n  local s = STOPS[f % #STOPS + 1]\n  if f == {} then w:follow(s[1], s[2], {{cut = true}}) else w:follow(s[1], s[2]) end\nend\n",
        &setup[..setup.find("function frame").unwrap()],
        stops.join(", "),
        JUMPS.len() - 1
    );
    let mut e = engine(&main, &tmj(MAP_W, MAP_H, &gids)).unwrap();
    for f in 0..JUMPS.len() as u32 * 2 {
        let lt = e
            .frame(f as f64 / 60.0, f)
            .unwrap_or_else(|err| panic!("frame {f}: {err:?}"));
        let (cx, cy) = JUMPS[f as usize % JUMPS.len()];
        let fb = render_frame(&lt, e.memory());
        for sy in 0..HEIGHT {
            for sx in 0..WIDTH {
                let p = &fb[(sy * WIDTH + sx) * 4..][..3];
                let want = reference(&sheet, &gids, cx as usize + sx, cy as usize + sy);
                assert_eq!(
                    [p[0] >> 3, p[1] >> 3, p[2] >> 3],
                    want,
                    "frame {f}, camera {cx},{cy}, pixel {sx},{sy}"
                );
            }
        }
    }
}

#[test]
fn world_load_is_a_file_level_placement() {
    let lib = "local world = require(\"@ppu/world\")\n";
    fails(
        &format!("{lib}function init() world.load(\"level1\") end function frame() end"),
        "world.load runs at file level",
    );
    fails(
        &format!("{lib}function frame() world.load(\"level1\") end"),
        "world.load runs at file level",
    );
    fails(
        &format!("{lib}local w = world.load(\"level2\") function frame() end"),
        "no file named level2.tmj",
    );
    fails(
        &format!(
            "{lib}local w = world.load(\"level1\") w:bind(\"ground\", bg[1]) function frame() end"
        ),
        "w:bind runs in init()",
    );
    fails(&format!("{lib}local w = world.load(\"level1\") function init() w:bind(\"ground\", bg[1]) end function frame() end"), "place it first, at file level, with dma(\"tiles\"");
    fails(
        &BOUND.replace("bind(\"ground\"", "bind(\"sky\""),
        "no tile layer named sky; it has ground",
    );
    fails(
        &BOUND.replace("w:bind", "bg[1].map_base = 0x1000 w:bind"),
        "overlaps 'tiles' in VRAM",
    );
}

#[test]
fn empty_cells_need_a_blank_tile_in_the_sheet() {
    // tiles.png's char 0 is blank, so an empty cell draws it.
    let mut data = vec![2u32; 64 * 32];
    data[0] = 0;
    let mut e = engine(&format!("{BOUND}function frame() end"), &tmj(64, 32, &data)).unwrap();
    e.frame(0.0, 0).unwrap();
    assert_eq!(e.memory().vram[0], 0, "the empty cell is char 0, palette 0");
    assert_eq!(e.memory().vram[1] & 0x3ff, 1, "gid 2 is char 1");
    // A sheet with no blank char refuses a map with empty cells.
    let mut e = LuaEngine::new();
    let full: Vec<u8> = sheet()
        .chunks(4)
        .map(|_| [8u8, 8, 8, 255])
        .flatten()
        .collect();
    add_sheet(&mut e, "tiles", full, 64, 16, 4);
    let map = tmj(64, 32, &data);
    let err = e
        .set_sources(&[
            ("main.luau", &format!("{BOUND}function frame() end")),
            ("level1.tmj", &map),
        ])
        .unwrap_err();
    assert!(err.message.contains("has no blank tile"), "{}", err.message);
}

/// A sheet of several palettes says which one each char uses in its payload (v4), so a map draws from it
/// without setSheetPalettes; a payload from before v4 does not, and is refused without them.
#[test]
fn a_multi_palette_sheet_without_its_palettes_is_refused() {
    let mut e = LuaEngine::new();
    let (mut payload, _) = ppu_core::convert_source(
        ppu_core::SourceKind::Sheet,
        &ppu_core::ConvertOptions {
            bit_depth: Some(4),
            ..Default::default()
        },
        &sheet(),
        64,
        16,
    )
    .unwrap();
    e.add_source("tiles", &payload.encode()).unwrap();
    let map = tmj(64, 32, &vec![1u32; 64 * 32]);
    e.set_sources(&[("main.luau", BOUND), ("level1.tmj", &map)])
        .unwrap();
    let ppu_core::SourcePayload::Sheet(sheet) = &mut payload else {
        panic!("a sheet")
    };
    assert!(
        sheet.palettes.len() > 1,
        "the fixture needs several palettes"
    );
    sheet.pals.clear();
    let mut e = LuaEngine::new();
    e.add_source("tiles", &payload.encode()).unwrap();
    let err = e
        .set_sources(&[("main.luau", BOUND), ("level1.tmj", &map)])
        .unwrap_err();
    assert!(err.message.contains("setSheetPalettes"), "{}", err.message);
}

#[test]
fn a_scene_redraws_its_world_when_entered_again() {
    let gids = gids();
    let map = tmj(MAP_W, MAP_H, &gids);
    let level = r#"local world = require("@ppu/world")
local tiles = dma("tiles", {char = 0x2000, pal = 32})
local w = world.load("level1")
local x = 0
local function init()
  mode = 1
  brightness = 15
  w:bind("ground", bg[1])
  w:follow(x, 40)
  screen.main.bg1 = true
end
local function frame(t, f)
  x += 8
  w:follow(x, 40)
  if f == 9 then scenes.go(PAUSE) end
end
return {init = init, frame = frame}
"#;
    let pause = "local function frame(t, f) if f == 2 then scenes.go(LEVEL) end end\nreturn {frame = frame}\n";
    let main =
        "LEVEL = scenes.add(require(\"./level\"))\nPAUSE = scenes.add(require(\"./pause\"))\n";
    let mut e = engine_with(
        &mut |e| add_sheet(e, "tiles", sheet(), 64, 16, 4),
        &[
            ("main.luau", main),
            ("level.luau", level),
            ("pause.luau", pause),
            ("level1.tmj", &map),
        ],
    );
    let sheet = sheet();
    let mut last = None;
    for f in 0..16u32 {
        last = Some(
            e.frame(f as f64 / 60.0, f)
                .unwrap_or_else(|err| panic!("frame {f}: {err:?}")),
        );
    }
    // Back in the level for 3 updates after a cold boot: x went 80, 88, 96, 104.
    let fb = render_frame(&last.unwrap(), e.memory());
    for sy in (0..HEIGHT).step_by(7) {
        for sx in 0..WIDTH {
            let p = &fb[(sy * WIDTH + sx) * 4..][..3];
            assert_eq!(
                [p[0] >> 3, p[1] >> 3, p[2] >> 3],
                reference(&sheet, &gids, 104 + sx, 40 + sy),
                "pixel {sx},{sy}"
            );
        }
    }
}

#[test]
fn a_narrow_map_streams_rows_into_one_32x32_screen() {
    let (sheet, all) = (sheet(), gids());
    // The first 32 columns of the big map, 64 rows tall.
    let gids: Vec<u32> = (0..32 * MAP_H)
        .map(|i| all[(i / 32) * MAP_W + i % 32])
        .collect();
    let main = r#"local world = require("@ppu/world")
local tiles = dma("tiles", {char = 0x2000, pal = 32})
local w = world.load("level1")
local y, dy = 0, 3
function init()
  mode = 1
  brightness = 15
  w:bind("ground", bg[1])
  w:follow(0, 0)
  screen.main.bg1 = true
end
function frame(t, f)
  if f > 0 then
    y += dy
    if y >= 288 then y, dy = 288, -3 end
  end
  w:follow(0, y + 0.5) -- fractions are dropped
end
"#;
    let mut e = engine(main, &tmj(32, MAP_H, &gids)).unwrap();
    let mut y: i64 = 0;
    let mut dy = 3;
    for f in 0..160u32 {
        if f > 0 {
            y += dy;
            if y >= 288 {
                y = 288;
                dy = -3;
            }
        }
        let lt = e.frame(f as f64 / 60.0, f).unwrap();
        assert_eq!(
            lt.rows[100].bg[0].screen_size, 0,
            "a 32-wide map is one screen"
        );
        if f % 11 != 0 {
            continue;
        }
        let fb = render_frame(&lt, e.memory());
        for sy in 0..HEIGHT {
            for sx in 0..WIDTH {
                let p = &fb[(sy * WIDTH + sx) * 4..][..3];
                assert_eq!(
                    [p[0] >> 3, p[1] >> 3, p[2] >> 3],
                    reference_w(&sheet, &gids, 32, sx, y as usize + sy),
                    "frame {f}, y {y}: pixel {sx},{sy}"
                );
            }
        }
    }
    assert!(y < 288 && dy < 0, "the camera turned at the bottom");
}

#[test]
fn a_follow_presents_only_the_strips_it_uncovered() {
    // The toy counts the strips presented: none while the camera holds still, at most a row and a
    // column a moving frame, and the whole window (29 rows) only on the follow after a bind.
    let main = r#"local world = require("@ppu/world")
local tiles = dma("tiles", {char = 0x1000, pal = 0})
local w = world.load("level1")
local real = __ppu_world_strip
local calls = 0
__ppu_world_strip = function(...) calls += 1 real(...) end
local x, y = 0, 0
function init()
  w:bind("ground", bg[1])
  w:follow(0, 0)
  assert(calls == 28, "the first follow draws the whole window: " .. calls)
end
function frame(t, f)
  calls = 0
  if f % 3 ~= 0 then x, y = x + 4, y + 3 end
  w:follow(x, y)
  if f % 3 == 0 then assert(calls == 0, "still camera, " .. calls .. " strips") end
  assert(calls <= 2, "frame " .. f .. ": " .. calls .. " strips")
end
"#;
    let mut e = engine(main, &tmj(64, 64, &vec![1u32; 64 * 64])).unwrap();
    for f in 0..90 {
        e.frame(f as f64 / 60.0, f)
            .unwrap_or_else(|err| panic!("frame {f}: {}", err.message));
    }
}

#[test]
fn a_map_refusal_names_the_map_file() {
    let map = tmj(64, 32, &vec![1u32; 64 * 32]).replace("\"orthogonal\"", "\"isometric\"");
    let mut e = LuaEngine::new();
    add_sheet(&mut e, "tiles", sheet(), 64, 16, 4);
    let err = e
        .set_sources(&[("main.luau", BOUND), ("level1.tmj", &map)])
        .unwrap_err();
    assert_eq!(err.file.as_deref(), Some("level1.tmj"));
    assert!(
        err.message.contains("isometric maps are not supported"),
        "{}",
        err.message
    );
}

// w:enter: several maps one world shows in turn on the same backgrounds.

/// Pseudo-random gids with flips, some empty, from `seed`.
fn seeded(n: usize, mut s: u32) -> Vec<u32> {
    (0..n)
        .map(|_| {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            match s % 20 {
                0 => 0,
                n => {
                    1 + (n % 15)
                        + if s & 0x100 != 0 { FLIP_H } else { 0 }
                        + if s & 0x200 != 0 { FLIP_V } else { 0 }
                }
            }
        })
        .collect()
}

/// A level named `name`: a `ground` layer (gid 4 classed solid; with `lava`, gid 10 classed lava) and, with `front`,
/// a `front` layer, both drawn from the sheet `image` names; one object named after the map.
struct Level {
    name: &'static str,
    width: usize,
    height: usize,
    ground: Vec<u32>,
    front: Option<Vec<u32>>,
    lava: bool,
    image: &'static str,
}

impl Level {
    fn new(name: &'static str, width: usize, height: usize, seed: u32) -> Level {
        Level {
            name,
            width,
            height,
            ground: seeded(width * height, seed),
            front: Some(seeded(width * height, seed ^ 0x5555)),
            lava: false,
            image: "tiles",
        }
    }

    fn tmj(&self) -> String {
        let mut tiles = vec![json!({"id": 3, "type": "solid"})];
        if self.lava {
            tiles.push(json!({"id": 9, "type": "lava"}));
        }
        let (w, h) = (self.width, self.height);
        let mut layers = vec![
            json!({"type": "tilelayer", "name": "ground", "width": w, "height": h, "data": self.ground}),
        ];
        if let Some(front) = &self.front {
            layers.push(json!({"type": "tilelayer", "name": "front", "width": w, "height": h, "data": front}));
        }
        layers.push(json!({"type": "objectgroup", "name": "things", "objects": [
            {"name": self.name, "type": "start", "x": 8, "y": 8, "width": 0, "height": 0}]}));
        json!({
            "orientation": "orthogonal", "infinite": false, "tilewidth": 8, "tileheight": 8, "width": w, "height": h,
            "tilesets": [{"firstgid": 1, "name": "cave", "image": format!("art/{}.png", self.image), "imagewidth": 64, "imageheight": 16,
                "tilewidth": 8, "tileheight": 8, "columns": 8, "tilecount": 16, "tiles": tiles}],
            "layers": layers
        })
        .to_string()
    }
}

fn levels_engine(main: &str, levels: &[&Level]) -> Result<LuaEngine, String> {
    let mut e = LuaEngine::new();
    add_sheet(&mut e, "tiles", sheet(), 64, 16, 4);
    add_sheet(&mut e, "other", sheet(), 64, 16, 4);
    let maps: Vec<(String, String)> = levels
        .iter()
        .map(|l| (format!("{}.tmj", l.name), l.tmj()))
        .collect();
    let mut files: Vec<(&str, &str)> = vec![("main.luau", main)];
    files.extend(maps.iter().map(|(n, t)| (n.as_str(), t.as_str())));
    e.set_sources(&files).map_err(|e| e.message)?;
    Ok(e)
}

/// The window cells a camera at (x, y) shows of `level`, as VRAM addresses from `base`.
fn window_cells(level: &Level, base: usize, x: usize, y: usize) -> Vec<usize> {
    let wide = level.width > 32;
    let mut out = vec![];
    for r in y / 8..=((y + 223) / 8).min(level.height - 1) {
        for c in x / 8..=(x / 8 + 32).min(level.width - 1) {
            out.push(base + if wide && c % 64 >= 32 { 0x400 } else { 0 } + (r % 32) * 32 + c % 32);
        }
    }
    out
}

const ENTERS: &str = r#"local world = require("@ppu/world")
local tiles = dma("tiles", {char = 0x2000, pal = 32})
local w = world.load("level1")
local level2 = world.load("level2")
local level3 = world.load("level3")
local x, y = 8, 8
-- Update -> the map entered and its camera; level1 again at 9, then a jump within it at 12.
local CUTS = {[3] = {"level2", 0, 40}, [6] = {"level3", 100, 200}, [9] = {"level1", 16, 8}, [12] = {"level1", 200, 24}}

function init()
  mode = 1
  brightness = 15
  bg[1].map_base = 0x0800
  w:bind("ground", bg[1])
  bg[2].map_base = 0x1000
  w:bind("front", bg[2])
  w:follow(x, y)
  screen.main.bg1 = true
end

function frame(t, f)
  force_blank = false
  -- The usual order: follow first, then the door check enters.
  w:follow(x, y)
  local cut = CUTS[f]
  if cut then
    force_blank = true
    x, y = cut[2], cut[3]
    w:enter(cut[1], x, y)
    assert(not w.cut, "cut is false when enter returns")
    -- A follow may come after the enter too.
    w:follow(x, y)
  end
  if f == 4 then
    assert(w.name == "level2" and w.width == 32 and w.height == 40, "level2's size")
    assert(w.objects[1].name == "level2" and w.kinds.solid == 1 and w.kinds.lava == nil, "level2's objects and kinds")
    assert(bg[1].screen_size == 0 and bg[2].screen_size == 0, "a 32-wide map is one screen")
  elseif f == 7 then
    assert(w.name == "level3" and w.width == 48 and w.height == 64 and w.objects[1].name == "level3", "level3")
    assert(w.kinds.lava == 2 and w:tile("ground", 0, 0) == w.kinds.lava, "level3's kinds and tiles")
    assert(bg[1].screen_size == 1, "a 48-wide map is two screens")
  elseif f == 10 then
    assert(w.name == "level1" and w.width == 64 and w.objects[1].name == "level1", "level1 again")
    assert(w.kinds.lava == nil and w:tile("ground", 0, 0) == w.kinds.solid, "level1's kinds and tiles")
  end
end
"#;

#[test]
fn enter_shows_another_map_whole_on_the_same_backgrounds() {
    let mut l1 = Level::new("level1", 64, 32, 0x1111_2222);
    l1.ground[0] = 4;
    let l2 = Level::new("level2", 32, 40, 0x3333_4444);
    let mut l3 = Level::new("level3", 48, 64, 0x5555_6666);
    l3.lava = true;
    l3.ground[0] = 10;
    let mut e = levels_engine(ENTERS, &[&l1, &l2, &l3]).unwrap();
    let cuts = [
        (3, &l2, 0, 40),
        (6, &l3, 100, 200),
        (9, &l1, 16, 8),
        (12, &l1, 200, 24),
    ];
    let sheet = sheet();
    for f in 0..15u32 {
        let lt = e
            .frame(f as f64 / 60.0, f)
            .unwrap_or_else(|err| panic!("frame {f}: {}", err.message));
        let fb = render_frame(&lt, e.memory());
        if cuts.iter().any(|c| c.0 == f) {
            assert!(
                fb.chunks(4).all(|p| p[..3] == [0, 0, 0]),
                "frame {f}: the enter update is black"
            );
        }
        let Some(&(_, level, x, y)) = cuts.iter().find(|c| c.0 + 1 == f) else {
            continue;
        };
        // The next update shows the map at the camera, straight from the sheet...
        for sy in 0..HEIGHT {
            for sx in 0..WIDTH {
                let p = &fb[(sy * WIDTH + sx) * 4..][..3];
                let want = reference_w(&sheet, &level.ground, level.width, x + sx, y + sy);
                assert_eq!(
                    [p[0] >> 3, p[1] >> 3, p[2] >> 3],
                    want,
                    "frame {f}, {} at {x},{y}: pixel {sx},{sy}",
                    level.name
                );
            }
        }
        // ...and both bound maps hold the window a fresh bind and follow of that map draws.
        let fresh = ENTERS
            .replace(
                "world.load(\"level1\"",
                &format!("world.load(\"{}\"", level.name),
            )
            .replace("local x, y = 8, 8", &format!("local x, y = {x}, {y}"));
        let fresh = fresh.replace("function frame(t, f)", "function frame(t, f) do return end");
        let mut g = levels_engine(&fresh, &[&l1, &l2, &l3]).unwrap();
        g.frame(0.0, 0).unwrap();
        for base in [0x0800, 0x1000] {
            for at in window_cells(level, base, x, y) {
                assert_eq!(
                    e.memory().vram[at],
                    g.memory().vram[at],
                    "frame {f}, {}: VRAM {at:#06x}",
                    level.name
                );
            }
        }
    }
}

/// `w:enter` refused with `needle`: the world is as it was (its fields, and the follow after it still streams).
fn enter_refused(levels: &[&Level], setup: &str, call: &str, needle: &str) {
    let main = format!(
        r#"local world = require("@ppu/world")
local tiles = dma("tiles", {{char = 0x2000, pal = 32}})
local other = dma("other", {{char = 0x3000, pal = 64}})
local w = world.load("level1")
{setup}
function init()
  mode = 1
  bg[1].map_base = 0x0800
  w:bind("ground", bg[1])
  bg[2].map_base = 0x1000
  w:bind("front", bg[2])
  w:follow(0, 0)
end
function frame(t, f)
  force_blank = true
  local ok, err = pcall(function() {call} end)
  assert(not ok, "the enter should be refused")
  assert(w.name == "level1" and w.id == 1 and w.width == 64, "a refused enter changes nothing")
  w:follow(f * 4, 0)
  -- Two updates streamed on after a refusal; the third shows its message.
  if f == 2 then error(err, 0) end
end
"#
    );
    let mut e = levels_engine(&main, levels).unwrap_or_else(|m| panic!("{needle}: {m}"));
    refused_on_frame_2(&mut e, needle);
}

fn refused_on_frame_2(e: &mut LuaEngine, needle: &str) {
    for f in 0..2 {
        e.frame(f as f64 / 60.0, f)
            .unwrap_or_else(|err| panic!("{needle}: frame {f}: {}", err.message));
    }
    let err = e
        .frame(2.0 / 60.0, 2)
        .err()
        .unwrap_or_else(|| panic!("{needle}: no refusal"))
        .message;
    assert!(err.contains(needle), "{needle:?} not in {err}");
}

#[test]
fn an_enter_is_refused_naming_the_cause_and_changes_nothing() {
    let l1 = Level::new("level1", 64, 32, 1);
    let l2 = Level::new("level2", 32, 40, 2);
    let both: &[&Level] = &[&l1, &l2];
    let load2 = r#"local level2 = world.load("level2")"#;
    enter_refused(
        both,
        load2,
        r#"force_blank = false w:enter("level2", 0, 0)"#,
        "set force_blank = true before it, and false on a later update",
    );
    enter_refused(
        both,
        load2,
        "w:enter(7, 0, 0)",
        "the first argument is a map's name",
    );
    enter_refused(
        both,
        load2,
        r#"w:enter("level9", 0, 0)"#,
        "the program loads no map named level9: load it at file level there",
    );
    enter_refused(
        both,
        load2,
        r#"w:enter("level2", 0, 200)"#,
        "the camera at 0, 200 leaves map level2",
    );
    for args in ["0", "0/0, 0", "\"0\", 0", "0, 0, 1"] {
        enter_refused(
            both,
            load2,
            &format!("w:enter(\"level2\", {args})"),
            "the camera's map pixel position, two finite numbers",
        );
    }
    let mut bare = Level::new("level2", 32, 40, 2);
    bare.front = None;
    enter_refused(
        &[&l1, &bare],
        load2,
        r#"w:enter("level2", 0, 0)"#,
        "map level2 has no tile layer named front, which bg[2] shows",
    );
    let mut moved = Level::new("level2", 32, 40, 2);
    moved.image = "other";
    enter_refused(&[&l1, &moved], load2, r#"w:enter("level2", 0, 0)"#, "draws from a sheet placed at char 0x3000, palette 4, but bg[1] shows one at char 0x2000, palette 2");
    // A narrow world's map at the end of VRAM, or just before a placement, has no room for a wide one.
    let (n1, w2) = (
        Level::new("level1", 32, 32, 1),
        Level::new("level2", 64, 32, 2),
    );
    for (base, needle) in [
        (
            "0x7c00",
            "bg[1].map_base must be a multiple of 0x400 with room for the 64x32 map before 0x8000",
        ),
        (
            "0x1c00",
            "the map at 0x1c00..0x2400 overlaps 'tiles' in VRAM",
        ),
    ] {
        enter_refused_at(
            &[&n1, &w2],
            load2,
            base,
            r#"w:enter("level2", 0, 0)"#,
            needle,
        );
    }
}

fn enter_refused_at(levels: &[&Level], setup: &str, base: &str, call: &str, needle: &str) {
    // The refusal helper's bg[1] at `base` (the other level is 32 wide, so it binds).
    let mut e = LuaEngine::new();
    add_sheet(&mut e, "tiles", sheet(), 64, 16, 4);
    let main = format!(
        r#"local world = require("@ppu/world")
local tiles = dma("tiles", {{char = 0x2000, pal = 32}})
local w = world.load("level1")
{setup}
function init()
  bg[1].map_base = {base}
  w:bind("ground", bg[1])
  w:follow(0, 0)
end
function frame(t, f)
  force_blank = true
  local ok, err = pcall(function() {call} end)
  assert(not ok and w.name == "level1", "a refused enter changes nothing")
  w:follow(0, f * 4)
  if f == 2 then error(err, 0) end
end
"#
    );
    let maps: Vec<(String, String)> = levels
        .iter()
        .map(|l| (format!("{}.tmj", l.name), l.tmj()))
        .collect();
    let mut files: Vec<(&str, &str)> = vec![("main.luau", &main)];
    files.extend(maps.iter().map(|(n, t)| (n.as_str(), t.as_str())));
    e.set_sources(&files)
        .unwrap_or_else(|m| panic!("{needle}: {}", m.message));
    refused_on_frame_2(&mut e, needle);
}

#[test]
fn a_world_on_the_mode7_plane_does_not_enter() {
    let mut e = LuaEngine::new();
    let picture: Vec<u8> = (0..128 * 128u32)
        .flat_map(|i| [(i % 128) as u8 * 2, (i / 128) as u8 * 2, 96, 255])
        .collect();
    common::add_m7(&mut e, "track", picture, 128, 128);
    let ring = json!({
        "orientation": "orthogonal", "tilewidth": 8, "tileheight": 8, "width": 64, "height": 64,
        "tilesets": [{"firstgid": 1, "name": "track", "image": "track.png", "imagewidth": 128, "imageheight": 128,
            "tilewidth": 8, "tileheight": 8, "columns": 16, "tilecount": 256}],
        "layers": [{"type": "tilelayer", "name": "road", "width": 64, "height": 64, "data": vec![1u32; 64 * 64]}]
    })
    .to_string();
    let main = r#"local world = require("@ppu/world")
local track = dma("track")
local w = world.load("ring")
function init()
  mode = 7
  w:bind("road", m7)
end
function frame(t, f)
  force_blank = true
  local ok, err = pcall(function() w:enter("ring", 0, 0) end)
  assert(not ok, "refused")
  if f == 2 then error(err, 0) end
end
"#;
    e.set_sources(&[("main.luau", main), ("ring.tmj", &ring)])
        .unwrap();
    refused_on_frame_2(
        &mut e,
        "a world on the Mode 7 plane (or loaded with radius or view) cannot enter another map",
    );
}

/// A scene's world that entered another map starts over on its own when the scene comes back: title -> level
/// (enter level2) -> title -> level (level1 again, then level2 again). A map only another scene loads is no target.
#[test]
fn bind_after_enter_restores_the_loaded_map() {
    let l1 = Level::new("level1", 64, 32, 7);
    let l2 = Level::new("level2", 32, 40, 8);
    let level = r#"local world = require("@ppu/world")
local tiles = dma("tiles", {char = 0x2000, pal = 32})
local w = world.load("level1")
local level2 = world.load("level2")
local n = 0
local function init()
  mode = 1
  brightness = 15
  n = 0
  bg[1].map_base = 0x0800
  w:bind("ground", bg[1])
  bg[2].map_base = 0x1000
  w:bind("front", bg[2])
  assert(w.name == "level1" and w.width == 64 and w.objects[1].name == "level1", "bind restores level1's fields")
  assert(bg[1].screen_size == 1, "and its screen size")
  w:follow(0, 0)
  screen.main.bg1 = true
end
local function frame(t, f)
  n += 1
  force_blank = false
  if n == 3 then
    force_blank = true
    local ok = pcall(function() w:enter("level3", 0, 0) end)
    assert(not ok, "level3 belongs to the title scene")
    w:enter("level2", 0, 40)
  elseif n == 6 then
    scenes.go(TITLE)
  end
end
return {init = init, frame = frame}
"#;
    let title = r#"local world = require("@ppu/world")
local level3 = world.load("level3")
local n = 0
local function init() n = 0 end
local function frame(t, f)
  n += 1
  if n == 3 then scenes.go(LEVEL) end
end
return {init = init, frame = frame}
"#;
    let main = "TITLE = scenes.add(require(\"./title\"))\nLEVEL = scenes.add(require(\"./level\"))\nscenes.go(LEVEL)\n";
    let l3 = Level::new("level3", 32, 32, 9);
    let (t1, t2, t3) = (l1.tmj(), l2.tmj(), l3.tmj());
    let mut e = engine_with(
        &mut |e| add_sheet(e, "tiles", sheet(), 64, 16, 4),
        &[
            ("main.luau", main),
            ("level.luau", level),
            ("title.luau", title),
            ("level1.tmj", &t1),
            ("level2.tmj", &t2),
            ("level3.tmj", &t3),
        ],
    );
    let sheet = sheet();
    let shows = |fb: &[u8], l: &Level, x: usize, y: usize| {
        (0..HEIGHT).step_by(5).all(|sy| {
            (0..WIDTH).all(|sx| {
                let p = &fb[(sy * WIDTH + sx) * 4..][..3];
                [p[0] >> 3, p[1] >> 3, p[2] >> 3]
                    == reference_w(&sheet, &l.ground, l.width, x + sx, y + sy)
            })
        })
    };
    let mut seen = vec![];
    for f in 0..24u32 {
        let lt = e
            .frame(f as f64 / 60.0, f)
            .unwrap_or_else(|err| panic!("frame {f}: {}", err.message));
        let fb = render_frame(&lt, e.memory());
        let what = if shows(&fb, &l1, 0, 0) {
            "level1"
        } else if shows(&fb, &l2, 0, 40) {
            "level2"
        } else if fb.chunks(4).all(|p| p[..3] == [0, 0, 0]) {
            "black"
        } else {
            "other"
        };
        if seen.last() != Some(&what) {
            seen.push(what);
        }
    }
    // Three rounds; the title scene draws nothing.
    let round = ["level1", "black", "level2", "black"];
    assert_eq!(seen, [round, round, round].concat()[..11], "{seen:?}");
}

/// A power cycle (Run from t=0) and a source added after load recompile the program from its
/// stored sources: its Tiled maps must come along, or world.load finds no file.
#[test]
fn a_power_cycle_and_a_new_source_keep_the_maps() {
    let mut e = engine(&main_luau(), &tmj(MAP_W, MAP_H, &gids())).unwrap();
    e.frame(0.0, 0).unwrap();
    e.reset()
        .unwrap_or_else(|err| panic!("reset: {}", err.message));
    e.frame(0.0, 0)
        .unwrap_or_else(|err| panic!("after reset: {}", err.message));
    add_sheet(&mut e, "tiles", sheet(), 64, 16, 4);
    e.frame(1.0 / 60.0, 1)
        .unwrap_or_else(|err| panic!("after a new source: {}", err.message));
}

/// The cells a w:set test changes in frame `f`: (map pixel x, y, tile id), one on screen and one off it (to the
/// right, while the map reaches there).
fn sets_at(f: i64) -> Vec<(i64, i64, u32)> {
    if f == 0 || f % 7 != 0 {
        return vec![];
    }
    let (x, y) = camera(f);
    let t = 1 + (f % 15) as u32;
    let mut out = vec![(x + f * 37 % 256, y + f * 53 % 224, t)];
    if x + 300 < (MAP_W * 8) as i64 {
        out.push((x + 300, y + f % 200, 16 - t));
    }
    out
}

/// w:set changes a cell: the screen shows it at once when on screen, it is still there after the camera scrolls
/// away and back (the path reverses), and w:tile reports the new tile's kind.
#[test]
fn a_set_cell_shows_now_and_stays_when_it_scrolls_back() {
    let (sheet, mut gids) = (sheet(), gids());
    let main = main_luau().replace(
        "  w:follow(x, y)\nend",
        r#"  w:follow(x, y)
  if f > 0 and f % 7 == 0 then
    local t = 1 + f % 15
    local KIND = {[3] = 1, [4] = 1, [9] = 2}
    local sx, sy = x + f * 37 % 256, y + f * 53 % 224
    w:set("ground", sx, sy, t)
    assert(w:tile("ground", sx, sy) == (KIND[t] or 0), "the kind of the tile set")
    if x + 300 < w.width * 8 then w:set("ground", x + 300, y + f % 200, 16 - t) end
  end
end"#,
    );
    assert!(main.contains("w:set"));
    let mut e = engine(&main, &tmj(MAP_W, MAP_H, &gids)).unwrap();
    let total: i64 = PATH.iter().map(|p| p.2).sum();
    for f in 0..=total {
        let lt = e
            .frame(f as f64 / 60.0, f as u32)
            .unwrap_or_else(|err| panic!("frame {f}: {err:?}"));
        for (x, y, t) in sets_at(f) {
            gids[(y as usize / 8) * MAP_W + x as usize / 8] = t + 1;
        }
        if f % 7 != 0 && f % 23 != 0 && f != total {
            continue;
        }
        let (cx, cy) = camera(f);
        let fb = render_frame(&lt, e.memory());
        for sy in 0..HEIGHT {
            for sx in 0..WIDTH {
                let p = &fb[(sy * WIDTH + sx) * 4..][..3];
                let want = reference(&sheet, &gids, cx as usize + sx, cy as usize + sy);
                assert_eq!(
                    [p[0] >> 3, p[1] >> 3, p[2] >> 3],
                    want,
                    "frame {f}, camera {cx},{cy}: screen pixel {sx},{sy}"
                );
            }
        }
    }
}

/// A 16 px map's w:set changes the whole Tiled tile, its 2x2 cells; w:bind starts the layer over as authored; a
/// tile id past the tileset, a pixel off the map and too many on-screen columns in one update are refused.
#[test]
fn a_set_changes_a_whole_tile_and_bind_restores_the_map() {
    let main = format!(
        "{}{}",
        BOUND,
        r#"assert(w:tile("ground", 40, 16) == 0, "authored")
w:set("ground", 40, 16, 3)
assert(w:tile("ground", 47, 23) == w.kinds.solid and w:tile("ground", 48, 16) == 0, "one cell")
function frame() end
"#
    );
    engine(&main, &tmj(64, 32, &vec![1u32; 64 * 32])).unwrap();
    // 16 px tiles: the same sheet read as 4x2 tiles of 2x2 chars.
    let map16 = tmj(32, 16, &vec![1u32; 32 * 16])
        .replace("\"tilewidth\":8", "\"tilewidth\":16")
        .replace("\"tileheight\":8", "\"tileheight\":16")
        .replace("\"columns\":8", "\"columns\":4")
        .replace("\"tilecount\":16", "\"tilecount\":8");
    assert!(map16.contains("\"tilecount\":8"), "{map16}");
    let main = format!(
        "{}{}",
        BOUND,
        r#"w:set("ground", 17, 33, 3)
for _, p in {{16, 32}, {31, 32}, {16, 47}, {31, 47}} do
  assert(w:tile("ground", p[1], p[2]) == w.kinds.solid, "the whole 16 px tile")
end
assert(w:tile("ground", 32, 32) == 0 and w:tile("ground", 15, 32) == 0, "only it")
assert(not pcall(function() w:set("ground", 0, 0, 8) end), "a tile past the 8 of the tileset")
function frame() end
"#
    );
    engine(&main, &map16).unwrap();
    // A bind starts the layer over: the set before it is gone.
    let main = r#"local world = require("@ppu/world")
local tiles = dma("tiles", {char = 0x1000, pal = 0})
local w = world.load("level1")
w:set("ground", 0, 0, 3)
assert(w:tile("ground", 0, 0) == w.kinds.solid, "set")
function init()
  w:bind("ground", bg[1])
  assert(w:tile("ground", 0, 0) == 0, "bind restores the authored map")
  w:follow(0, 0)
end
function frame() end
"#;
    engine(main, &tmj(64, 32, &vec![1u32; 64 * 32])).unwrap();
    for (call, needle) in [
        (
            "w:set(\"ground\", 0, 0, 16)",
            "tile 16 is not in layer ground's tileset, whose ids are 0 to 15",
        ),
        (
            "w:set(\"ground\", 0, 9999, 1)",
            "the pixel 0, 9999 is off the 512x256 px map",
        ),
        (
            "w:set(\"nope\", 0, 0, 1)",
            "the map has no tile layer named nope",
        ),
    ] {
        fails(&format!("{BOUND}{call}\nfunction frame() end"), needle);
    }
    fails(
        &format!("{BOUND}function frame(t, f)\n  w:follow(0, 0)\n  if f == 1 then for i = 0, 16 do w:set(\"ground\", i * 8, 0, 3) end end\nend"),
        "redraws more than 16 columns",
    );
}
