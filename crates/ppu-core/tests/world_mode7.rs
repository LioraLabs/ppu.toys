//! @ppu/world on the Mode 7 plane: a map drawn from an `m7` source binds with `w:bind(name, m7)`,
//! loads whole into the 128x128 plane (only each VRAM word's low byte), reads kinds and objects in
//! plane pixels, and with an EXTBG source turns on BG2's per-pixel priority.
mod common;

use common::{add_obj, px};
use ppu_core::{convert_source_with_priority, render_frame, ConvertOptions, LuaEngine, SourceKind};
use serde_json::json;

/// A 32x16 px tileset image, eight 8x8 cells: 0 blank, 1 floor (green), 2 pillar (red, its
/// priority-mask pixels high), 3 kerb (white), 4 the pillar's colour with a low mask (a tile of its
/// own because priority counts in identity), 5..7 other colours.
fn image() -> (Vec<u8>, Vec<u8>) {
    let colors: [[u8; 3]; 8] = [
        [0; 3],
        [40, 200, 64],
        [232, 48, 40],
        [248, 248, 248],
        [232, 48, 40],
        [40, 80, 232],
        [240, 200, 40],
        [160, 40, 200],
    ];
    let (mut rgba, mut mask) = (vec![0u8; 32 * 16 * 4], vec![0u8; 32 * 16 * 4]);
    for c in 1..8 {
        for y in 0..8 {
            for x in 0..8 {
                let at = (((c / 4) * 8 + y) * 32 + (c % 4) * 8 + x) * 4;
                rgba[at..at + 4].copy_from_slice(&[colors[c][0], colors[c][1], colors[c][2], 255]);
                if c == 2 {
                    mask[at..at + 4].copy_from_slice(&[255; 4]);
                }
            }
        }
    }
    (rgba, mask)
}

fn add_track(e: &mut LuaEngine, extbg: bool) {
    let (rgba, mask) = image();
    let opts = ConvertOptions {
        extbg: Some(extbg),
        ..Default::default()
    };
    let (payload, _) = convert_source_with_priority(
        SourceKind::M7,
        &opts,
        &rgba,
        32,
        16,
        extbg.then_some(&mask[..]),
    )
    .unwrap();
    e.add_source("track", &payload.encode()).unwrap();
}

/// A `w` x `h` cell map: cell (x, y) is tileset cell `cell(x, y)` (0 empty); floor cells are class
/// road, pillars wall.
fn tmj(w: usize, h: usize, cell: impl Fn(usize, usize) -> u32) -> String {
    let data: Vec<u32> = (0..w * h)
        .map(|i| cell(i % w, i / w))
        .map(|c| if c == 0 { 0 } else { c + 1 })
        .collect();
    json!({
        "orientation": "orthogonal", "infinite": false, "tilewidth": 8, "tileheight": 8, "width": w, "height": h,
        "tilesets": [{
            "firstgid": 1, "name": "track", "image": "track.png", "imagewidth": 32, "imageheight": 16,
            "tilewidth": 8, "tileheight": 8, "columns": 4, "tilecount": 8,
            "tiles": [{"id": 1, "type": "road"}, {"id": 2, "type": "wall"}]
        }],
        "layers": [
            {"type": "tilelayer", "name": "track", "width": w, "height": h, "data": data},
            {"type": "objectgroup", "name": "things", "objects": [
                {"name": "start", "type": "spawn", "x": 300, "y": 280, "width": 0, "height": 0}
            ]}
        ]
    })
    .to_string()
}

/// Floor everywhere, a pillar at cell (2, 2), the low-mask twin at (4, 2), empty cells on row 6.
fn cell(x: usize, y: usize) -> u32 {
    match (x, y) {
        (2, 2) => 2,
        (4, 2) => 4,
        (_, 6) => 0,
        _ => 1 + (x + y) as u32 % 2 * 2,
    }
}

const A: u16 = 1 << 4;
const B: u16 = 1 << 5;

const MAIN: &str = r#"
local world = require("@ppu/world")
local plane = dma("track")
local dot = dma("dot", {char = 0x4000, pal = 0})
local w = world.load("level1")
function init()
  mode = 7
  m7.a, m7.d = 1, 1
  w:bind("track", m7)
  screen.main.bg1 = true
  screen.main.bg2 = true
  screen.main.obj = true
  obj.char_base = 0x4000
  obj[0].x, obj[0].y, obj[0].on, obj[0].tile = 16, 16, true, dot.tile + dot.cells[1].tile
  road = w:tile("track", 300, 280) == w.kinds.road
  wall = w:tile("track", 2 * 8 + 3, 2 * 8 + 7) == w.kinds.wall
  start = w.objects[1].x
end
function frame(t, f)
  -- a: in front of the pillar; b: over the floor instead.
  obj[0].prio = if pad.a then 3 else 1
  obj[0].x = if pad.b then 48 else 16
end
function probe()
  return {road and 1 or 0, wall and 1 or 0, start, m7.extbg and 1 or 0}
end
"#;

/// The pixel at (x, y) is `rgb` as RGB555 shows it (each channel within 8).
fn near(fb: &[u8], x: usize, y: usize, rgb: [u8; 3]) -> bool {
    px(fb, x, y)[..3]
        .iter()
        .zip(rgb)
        .all(|(&a, b)| a.abs_diff(b) <= 8)
}

fn engine(extbg: bool, main: &str, map: &str) -> Result<LuaEngine, String> {
    let mut e = LuaEngine::new();
    add_track(&mut e, extbg);
    add_obj(&mut e, "dot", vec![255; 8 * 8 * 4], 8, 8);
    e.set_sources(&[("main.luau", main), ("level1.tmj", map)])
        .map_err(|e| e.message)?;
    Ok(e)
}

/// road, wall, the start object's x, m7.extbg.
fn probe(e: &mut LuaEngine) -> Vec<i64> {
    e.call_ints("probe", &[]).unwrap()
}

#[test]
fn binding_loads_the_whole_plane_into_the_low_bytes_and_keeps_the_tiles() {
    let map = tmj(40, 24, cell);
    let mut bound = engine(false, MAIN, &map).unwrap();
    let mut placed = engine(false, &MAIN.replace("w:bind(\"track\", m7)", ""), &map).unwrap();
    bound.frame(0.0, 0).unwrap();
    placed.frame(0.0, 0).unwrap();
    let (vb, vp) = (bound.memory().vram.to_vec(), placed.memory().vram.to_vec());
    // The tiles' pixels (high bytes) are the dma placement's, untouched.
    assert!(
        (0..0x4000).all(|i| vb[i] >> 8 == vp[i] >> 8),
        "a map write reached a pixel byte"
    );
    // The cell under the image's own top-left cell (blank) is the tile every empty or outside cell draws.
    let blank = vp[0] & 0xff;
    let tile_of = |c: usize| vp[(c / 4) * 128 + c % 4] & 0xff;
    for y in 0..128 {
        for x in 0..128 {
            let want = if x < 40 && y < 24 {
                tile_of(cell(x, y) as usize)
            } else {
                blank
            };
            assert_eq!(vb[y * 128 + x] & 0xff, want, "plane cell {x},{y}");
        }
    }
    assert_eq!(
        tile_of(2),
        tile_of(4),
        "without EXTBG the pillar and its twin are one tile"
    );
    assert_eq!(probe(&mut bound)[3], 0);
}

#[test]
fn kinds_and_objects_are_in_plane_pixels() {
    let mut e = engine(false, MAIN, &tmj(40, 40, cell)).unwrap();
    e.frame(0.0, 0).unwrap();
    assert_eq!(probe(&mut e)[..3], [1, 1, 300]);
}

#[test]
fn an_extbg_tileset_turns_on_bg2_and_a_sprite_passes_behind_and_in_front() {
    let mut e = engine(true, MAIN, &tmj(40, 24, cell)).unwrap();
    let lt = e.frame(0.0, 0).unwrap();
    assert_eq!(probe(&mut e)[3], 1);
    let v = e.memory().vram.to_vec();
    assert_ne!(
        v[2 * 128 + 2] & 0xff,
        v[2 * 128 + 4] & 0xff,
        "priority bits count in tile identity"
    );
    // Identity matrix: plane pixel = screen pixel. The sprite at 16,16 covers the pillar cell (2, 2).
    let behind = render_frame(&lt, e.memory());
    assert!(
        near(&behind, 19, 19, [232, 48, 40]),
        "priority 1: the pillar's high pixels cover the sprite"
    );
    e.set_pad(A);
    let lt = e.frame(1.0 / 60.0, 1).unwrap();
    let front = render_frame(&lt, e.memory());
    assert!(
        near(&front, 19, 19, [255, 255, 255]),
        "priority 3: the sprite is in front"
    );
    // And the floor (low priority) never covers it.
    e.set_pad(B);
    let lt = e.frame(2.0 / 60.0, 2).unwrap();
    assert!(
        near(&render_frame(&lt, e.memory()), 51, 19, [255, 255, 255]),
        "the floor is behind a priority-1 sprite"
    );
}

#[test]
fn what_the_plane_cannot_hold_is_refused() {
    let err =
        |extbg: bool, main: &str, map: &str| engine(extbg, main, map).err().unwrap_or_default();
    let m = err(false, MAIN, &tmj(130, 8, |_, _| 1));
    assert!(
        m.contains("130x8 cells") && m.contains("128x128") && m.contains("radius = r"),
        "{m}"
    );
    let m = err(
        false,
        &MAIN.replace("local plane = dma(\"track\")", ""),
        &tmj(8, 8, |_, _| 1),
    );
    assert!(m.contains("dma(\"track\")"), "{m}");
    let m = err(
        false,
        &MAIN.replace("w:bind(\"track\", m7)", "w:bind(\"track\", bg[1])"),
        &tmj(8, 8, |_, _| 1),
    );
    assert!(m.contains("w:bind(\"track\", m7)"), "{m}");
}

#[test]
fn a_tileset_over_256_distinct_tiles_is_refused_with_the_count() {
    // 17x16 cells of 8x8 px, each its own grey ramp: 272 distinct tiles.
    let (w, h) = (17 * 8, 16 * 8);
    let mut rgba = vec![255u8; w * h * 4];
    for (i, p) in rgba.chunks_exact_mut(4).enumerate() {
        let (x, y) = (i % w, i / w);
        // Two colours; pixel j of tile c is white when bit j of c is set: every tile its own.
        let c = y / 8 * 17 + x / 8;
        let j = (y % 8) * 8 + x % 8;
        let v = if j < 16 && c >> j & 1 == 1 { 255 } else { 0 };
        p[..3].copy_from_slice(&[v, v, 64]);
    }
    let (payload, _) = convert_source_with_priority(
        SourceKind::M7,
        &ConvertOptions::default(),
        &rgba,
        w as u32,
        h as u32,
        None,
    )
    .unwrap();
    let mut e = LuaEngine::new();
    e.add_source("track", &payload.encode()).unwrap();
    add_obj(&mut e, "dot", vec![255; 8 * 8 * 4], 8, 8);
    let map = tmj(8, 8, |_, _| 1)
        .replace("\"imagewidth\":32", "\"imagewidth\":136")
        .replace("\"imageheight\":16", "\"imageheight\":128")
        .replace("\"columns\":4", "\"columns\":17")
        .replace("\"tilecount\":8", "\"tilecount\":272");
    let m = e
        .set_sources(&[("main.luau", MAIN), ("level1.tmj", &map)])
        .err()
        .map(|e| e.message)
        .unwrap_or_default();
    assert!(m.contains("272 distinct") && m.contains("256"), "{m}");
}

const STREAM: &str = r#"
local world = require("@ppu/world")
local plane = dma("track")
local w = world.load("level1", {radius = RADIUS})
local n = 0
function init()
  mode = 7
  w:bind("track", m7)
  w:follow(START_X, START_Y)
end
-- Diagonal at full speed, reversing every 37 frames.
function frame(t, f)
  n += 1
  local k = (n // 37) % 2 == 0 and 1 or -1
  w:follow(w.x + k * SPEED, w.y + k * SPEED)
end
function probe()
  return {w.x, w.y}
end
"#;

fn stream_main(speed: u32, radius: u32, x: u32, y: u32) -> String {
    STREAM
        .replace("SPEED", &speed.to_string())
        .replace("RADIUS", &radius.to_string())
        .replace("START_X", &x.to_string())
        .replace("START_Y", &y.to_string())
}

fn tile_cell(x: usize, y: usize) -> u32 {
    (x * 7 + y * 13) as u32 % 8
}

#[test]
fn a_streamed_map_keeps_every_window_cell_in_the_plane_through_diagonal_moves_and_reversals() {
    keeps_every_window_cell(&stream_main(4, 20, 806, 1006));
    // Two cells a frame on each axis, and jumps past the whole 40-cell window and back.
    keeps_every_window_cell(&stream_main(16, 20, 806, 1006));
    keeps_every_window_cell(&stream_main(4, 20, 806, 1006).replace(
        "w:follow(w.x + k * 4, w.y + k * 4)",
        "w:follow(if n % 2 == 0 then 806 else 1206, if n % 3 == 0 then 1006 else 1700)",
    ));
}

fn keeps_every_window_cell(main: &str) {
    let (mw, mh, r) = (200usize, 300usize, 20usize);
    let map = tmj(mw, mh, tile_cell);
    let mut e = engine(false, main, &map).unwrap();
    let mut placed = engine(
        false,
        &MAIN.replace("w:bind(\"track\", m7)", ""),
        &tmj(8, 8, tile_cell),
    )
    .unwrap();
    placed.frame(0.0, 0).unwrap();
    let vp = placed.memory().vram.to_vec();
    let want = |c: u32| vp[(c as usize / 4) * 128 + c as usize % 4] & 0xff;
    let mut crossed_both = false;
    let mut last = (806, 1006);
    for f in 0..200 {
        e.frame(f as f64 / 60.0, f).unwrap();
        let p = probe(&mut e);
        let (x, y) = (p[0] as usize, p[1] as usize);
        crossed_both |= x / 8 != last.0 / 8 && y / 8 != last.1 / 8;
        last = (x, y);
        let vram = &e.memory().vram;
        for cy in y / 8 - r..y / 8 + r {
            for cx in x / 8 - r..x / 8 + r {
                assert_eq!(
                    vram[(cy % 128) * 128 + cx % 128] & 0xff,
                    want(tile_cell(cx, cy)),
                    "frame {f} at {x},{y}: cell {cx},{cy}"
                );
            }
        }
    }
    assert!(
        crossed_both,
        "the path never crossed a row and a column in one frame"
    );
}

#[test]
fn the_largest_streamed_map_loads_and_fills() {
    let map = tmj(512, 512, tile_cell);
    let mut e = engine(false, &stream_main(4, 53, 53 * 8 + 3, 400 * 8), &map).unwrap();
    e.frame(0.0, 0).unwrap();
    assert_eq!(probe(&mut e), [53 * 8 + 3 + 4, 400 * 8 + 4]);
}

#[test]
fn streaming_refusals_name_the_problem() {
    let load = |main: &str, w: usize, h: usize| {
        engine(false, main, &tmj(w, h, tile_cell))
            .err()
            .unwrap_or_default()
    };
    let ok = |main: &str, w: usize, h: usize| engine(false, main, &tmj(w, h, tile_cell)).is_ok();
    let m = load(MAIN, 200, 300);
    assert!(m.contains("200x300") && m.contains("radius = r"), "{m}");
    let m = load(
        &MAIN.replace(
            "world.load(\"level1\")",
            "world.load(\"level1\", {radius = 10})",
        ),
        128,
        128,
    );
    assert!(
        m.contains("loads whole") && m.contains("drop radius"),
        "{m}"
    );
    let m = load(&stream_main(1, 65, 600, 600), 200, 300);
    assert!(m.contains("1 to 64") && m.contains("65"), "{m}");
    // The radius is held only to the plane, at every step: what streaming costs is the cartridge's frame budget.
    assert!(ok(&stream_main(8, 64, 600, 600), 200, 300));
    assert!(ok(&stream_main(4, 53, 600, 600), 200, 300));
    let m = load(&stream_main(4, 20, 600, 600), 513, 200);
    assert!(m.contains("512x512") && m.contains("513x200"), "{m}");
    let m = load(&stream_main(4, 20, 600, 600), 30, 200);
    assert!(m.contains("2 * radius = 40") && m.contains("30x200"), "{m}");
    // The follow point must keep the window inside the map: x in 160..1447.
    let m = load(&stream_main(4, 20, 100, 600), 200, 300);
    assert!(m.contains("clamp it") && m.contains("160..1447"), "{m}");
    // speed belongs to a view: a streamed map ignores it.
    assert!(ok(
        &stream_main(4, 20, 600, 600).replace("{radius", "{speed = 4, radius"),
        200,
        300
    ));
    // A streamed plane and a background are one or the other, in either order.
    let both = |a: &str, b: &str| {
        let main = stream_main(4, 20, 600, 600)
            .replace("  w:bind(\"track\", m7)\n", &format!("  {a}\n  {b}\n"));
        load(&main, 200, 300)
    };
    // (This map's one layer is Mode 7, so a background first is refused by the engine as before.)
    let m = both("w:bind(\"track\", m7)", "w:bind(\"track\", bg[1])");
    assert!(m.contains("not both"), "{m}");
    // The library's refusal carries the world code, which opens its note.
    assert_eq!(
        ppu_core::engine_code(&m),
        Some(ppu_core::codes::WORLD),
        "{m}"
    );
    assert!(both("w:bind(\"track\", bg[1])", "w:bind(\"track\", m7)").contains("Mode 7 tileset"));
}

// A perspective view world: the follow point drives along a heading and the whole plane chases a
// window that leads it. HEADING(f) and STEP(f) are Luau expressions of the frame number f.
const VIEW: &str = r#"
local world = require("@ppu/world")
local plane = dma("track")
local w = world.load("level1", {speed = SPEED, view = {ahead = AHEAD, behind = BEHIND}})
local h, n = 0, 0
local x, y = START_X, START_Y
function init()
  mode = 7
  w:bind("track", m7)
  w:follow(x, y, h)
end
function frame(t, f)
  n += 1
  h = (HEADING) % 256
  local s = STEP
  local vx = math.floor(s * math.sin(h * math.pi / 128) + 0.5)
  local vy = math.floor(-s * math.cos(h * math.pi / 128) + 0.5)
  -- Bounce off a box inside the near bounds: an instant flip.
  if x + vx < LO_X or x + vx > HI_X or y + vy < LO_Y or y + vy > HI_Y then
    bounces += 1
    h = (h + 128) % 256
    vx, vy = -vx, -vy
  end
  x, y = x + vx, y + vy
  w:follow(x, y, h)
end
bounces = 0
function probe()
  return {w.x, w.y, w.c0, w.r0, h, w.near, w.catch_up, bounces, w.radius, w.ahead, w.behind}
end
"#;

struct View<'a> {
    speed: i64,
    ahead: i64,
    behind: i64,
    start: (i64, i64),
    heading: &'a str,
    step: &'a str,
    bounds: (i64, i64, i64, i64),
}

fn view_main(v: &View) -> String {
    VIEW.replace("SPEED", &v.speed.to_string())
        .replace("AHEAD", &v.ahead.to_string())
        .replace("BEHIND", &v.behind.to_string())
        .replace("START_X", &v.start.0.to_string())
        .replace("START_Y", &v.start.1.to_string())
        // H in HEADING is the previous frame's heading; f is the frame.
        .replace("HEADING", &v.heading.replace("H", "h"))
        .replace("STEP", v.step)
        .replace("LO_X", &v.bounds.0.to_string())
        .replace("HI_X", &v.bounds.1.to_string())
        .replace("LO_Y", &v.bounds.2.to_string())
        .replace("HI_Y", &v.bounds.3.to_string())
}

/// Six visibly distinct tiles (2 and 4 are one tile without EXTBG), hashed so no cell 128 away
/// in either axis is reliably the same: a stale plane slot shows.
fn view_cell(x: usize, y: usize) -> u32 {
    let h = (x as u32).wrapping_mul(73_856_093) ^ (y as u32).wrapping_mul(19_349_663);
    [1, 2, 3, 5, 6, 7][(h % 6) as usize]
}

/// The VRAM low byte each tileset cell draws as.
fn tile_bytes() -> impl Fn(u32) -> u16 {
    let mut placed = engine(
        false,
        &MAIN.replace("w:bind(\"track\", m7)", ""),
        &tmj(8, 8, tile_cell),
    )
    .unwrap();
    placed.frame(0.0, 0).unwrap();
    let vp = placed.memory().vram.to_vec();
    move |c: u32| vp[(c as usize / 4) * 128 + c as usize % 4] & 0xff
}

/// The target cell world.luau computes for a follow point and heading.
fn target(v: &View, x: i64, y: i64, h: i64, (mw, mh): (i64, i64)) -> (i64, i64) {
    let d = (4 * (v.ahead - v.behind)) as f64;
    let a = h as f64 * std::f64::consts::PI / 128.0;
    let dx = (d * a.sin() + 0.5).floor() as i64;
    let dy = (-d * a.cos() + 0.5).floor() as i64;
    (
        (x + dx).div_euclid(8).clamp(64, mw - 64),
        (y + dy).div_euclid(8).clamp(64, mh - 64),
    )
}

/// Drives `frames` frames, checking after each that the near square and the loaded window are
/// exact in the plane and the loaded centre stays within d of the camera. Returns per frame
/// (follow point, loaded centre, target), and the bounces (instant flips) the drive made.
type Run = Vec<((i64, i64), (i64, i64), (i64, i64))>;
fn drive(v: &View, frames: u32) -> (Run, i64) {
    let (mw, mh) = (300usize, 400usize);
    let mut e = engine(false, &view_main(v), &tmj(mw, mh, view_cell)).unwrap();
    let want = tile_bytes();
    let d = ((v.ahead - v.behind).abs() + 1) / 2;
    let mut out = Vec::new();
    for f in 0..frames {
        e.frame(f as f64 / 60.0, f).unwrap();
        let p = probe(&mut e);
        let (x, y, c0, r0, h, near) = (p[0], p[1], p[2], p[3], p[4], p[5]);
        assert_eq!((near, p[8], p[9], p[10]), (64 - d, 64, v.ahead, v.behind));
        let (lx, ly) = (c0 + 64, r0 + 64);
        let (fx, fy) = (x.div_euclid(8), y.div_euclid(8));
        assert!(
            (lx - fx).abs() <= d && (ly - fy).abs() <= d,
            "frame {f}: centre {lx},{ly} more than {d} from the camera cell {fx},{fy}"
        );
        let vram = &e.memory().vram;
        let check = |cx: i64, cy: i64, what: &str| {
            let (cx, cy) = (cx as usize, cy as usize);
            assert_eq!(
                vram[(cy % 128) * 128 + cx % 128] & 0xff,
                want(view_cell(cx, cy)),
                "frame {f} at {x},{y} heading {h}: {what} cell {cx},{cy}"
            );
        };
        for cy in fy - near..fy + near {
            for cx in fx - near..fx + near {
                check(cx, cy, "near");
            }
        }
        for cy in r0..r0 + 128 {
            for cx in c0..c0 + 128 {
                check(cx, cy, "window");
            }
        }
        out.push(((x, y), (lx, ly), target(v, x, y, h, (mw as i64, mh as i64))));
    }
    let bounces = probe(&mut e)[7];
    (out, bounces)
}

#[test]
fn a_turning_drive_keeps_the_near_square_and_the_loaded_window_exact_every_frame() {
    // Near 24 cells: x in 192..2215, y in 192..3015. Steady turns, instant flips every 50 frames
    // in one phase, a fast spin, at up to 4 px an axis, in a box near the corner and the near
    // bounds so the drive bounces (more instant flips) and the target clamps to the map's edge.
    let heading = "if f < 120 then H + 2 elseif f < 240 then H - 3 elseif f < 360 then (if f % 50 == 0 then H + 128 else H) \
                   elseif f < 420 then H + 37 else H + (f // 7 % 3 - 1) * 3";
    // Inside the map (the window lags far), then near a corner (bounces, the target clamps),
    // and with the target behind the camera.
    let (mut bounced, mut clamped) = (0, 0);
    for (ahead, behind, start, bounds) in [
        (96, 16, (1200, 1600), (300, 2100, 300, 2900)),
        (96, 16, (400, 400), (192, 640, 192, 640)),
        (16, 96, (1200, 1600), (300, 2100, 300, 2900)),
        (16, 96, (400, 400), (192, 640, 192, 640)),
    ] {
        let v = View {
            speed: 4,
            ahead,
            behind,
            start,
            heading,
            step: "4",
            bounds,
        };
        let (run, bounces) = drive(&v, 600);
        let lagged = run.iter().filter(|(_, l, t)| l != t).count();
        let lag = run
            .iter()
            .map(|(_, l, t)| (l.0 - t.0).abs().max((l.1 - t.1).abs()))
            .max()
            .unwrap();
        let edge = run
            .iter()
            .filter(|(_, _, t)| t.0 == 64 || t.1 == 64)
            .count();
        eprintln!("turning drive {ahead}/{behind} from {start:?}: {lagged} of 600 frames lag the target, at most {lag} cells; {bounces} bounces; {edge} frames clamped");
        assert!(
            lagged > 50 && lag > 20,
            "the drive never made the window lag far ({lagged} frames, {lag} cells)"
        );
        assert!(
            run.windows(2).filter(|p| p[0].0 != p[1].0).count() > 550,
            "the camera moved"
        );
        (bounced, clamped) = (bounced + bounces, clamped + edge);
    }
    assert!(
        bounced > 1 && clamped > 0,
        "the drives never reached the edge ({bounced} bounces, {clamped} clamped)"
    );
}

/// Settles heading 0 (up the plane), flips at frame `flip` to 128 and drives flat out down, away
/// from the lag: returns (updates until the centre reaches the target, w.catch_up).
fn catch_up(speed: i64, ahead: i64, behind: i64) -> (usize, i64) {
    let flip = 30;
    let heading = format!("if f < {flip} then 0 else 128");
    let v = View {
        speed,
        ahead,
        behind,
        start: (1200, 1200),
        heading: &heading,
        step: &speed.to_string(),
        bounds: (0, 9999, 0, 9999),
    };
    let (run, _) = drive(&v, flip + 100);
    assert_eq!(
        run[flip as usize - 1].1,
        run[flip as usize - 1].2,
        "settled before the flip"
    );
    let n = run[flip as usize..]
        .iter()
        .position(|(_, l, t)| l == t)
        .unwrap()
        + 1;
    assert!(
        run[flip as usize + n - 1..].iter().all(|(_, l, t)| l == t),
        "it stays caught up"
    );
    let mut e = engine(false, &view_main(&v), &tmj(300, 400, view_cell)).unwrap();
    e.frame(0.0, 0).unwrap();
    (n, probe(&mut e)[6])
}

#[test]
fn far_rows_catch_up_within_catch_up_updates_of_a_flip() {
    for (speed, ahead, behind) in [(4, 96, 16), (8, 58, 0)] {
        let (n, bound) = catch_up(speed, ahead, behind);
        eprintln!(
            "speed {speed}, view {ahead}/{behind}: caught up in {n} updates, catch_up = {bound}"
        );
        assert!(
            n as i64 <= bound,
            "speed {speed}: {n} updates, past catch_up {bound}"
        );
        assert!(
            n as i64 * 4 >= bound * 3,
            "speed {speed}: {n} updates is far inside {bound}: the drive is not adversarial"
        );
    }
}

#[test]
fn view_refusals_name_the_problem() {
    let base = View {
        speed: 4,
        ahead: 96,
        behind: 16,
        start: (1200, 1600),
        heading: "H",
        step: "0",
        bounds: (0, 9999, 0, 9999),
    };
    let main = |f: &dyn Fn(String) -> String| f(view_main(&base));
    let load = |main: &str, map: &str| engine(false, main, map).err().unwrap_or_default();
    let big = tmj(300, 400, view_cell);
    let refuse = |m: String, parts: &[&str]| assert!(parts.iter().all(|p| m.contains(p)), "{m}");
    refuse(
        load(
            &main(&|s| s.replace("ahead = 96, behind = 16", "ahead = 100, behind = 21")),
            &big,
        ),
        &["121", "120"],
    );
    refuse(
        load(
            &main(&|s| {
                s.replace("speed = 4", "speed = 8")
                    .replace("ahead = 96, behind = 16", "ahead = 110, behind = 0")
            }),
            &big,
        ),
        &["111 updates", "more than a second"],
    );
    refuse(
        load(
            &main(&|s| s.replace("speed = 4,", "speed = 4, radius = 20,")),
            &big,
        ),
        &["view sets the window: drop radius"],
    );
    refuse(
        load(&main(&|s| s), &tmj(128, 128, view_cell)),
        &["loads whole: drop view"],
    );
    refuse(
        load(&main(&|s| s), &tmj(300, 100, view_cell)),
        &["at least 128", "300x100"],
    );
    refuse(
        load(&main(&|s| s), &tmj(513, 300, view_cell)),
        &["at most 512", "513x300"],
    );
    // A map whose only layer is objects has no Mode 7 layer.
    let mut objects: serde_json::Value = serde_json::from_str(&tmj(300, 400, view_cell)).unwrap();
    objects["layers"].as_array_mut().unwrap().remove(0);
    refuse(
        load(
            &main(&|s| s.replace("w:bind(\"track\", m7)", "")),
            &objects.to_string(),
        ),
        &["no Mode 7 layer: drop view"],
    );
    refuse(
        load(
            &main(&|s| s.replace("view = {ahead = 96, behind = 16}", "view = 3")),
            &big,
        ),
        &["view is a table"],
    );
    refuse(
        load(&main(&|s| s.replace("ahead = 96", "ahead = -1")), &big),
        &["view.ahead", "whole number", "-1"],
    );
    refuse(
        load(&main(&|s| s.replace("behind = 16", "behind = 2.5")), &big),
        &["view.behind", "2.5"],
    );
    refuse(
        load(
            &main(&|s| s.replace("behind = 16", "behind = 16, side = 2")),
            &big,
        ),
        &["ahead and behind only", "side"],
    );
    refuse(
        load(
            &main(&|s| s.replace("speed = 4,", "speed = 4, veiw = 1,")),
            &big,
        ),
        &[
            "options are radius, view (and its speed), period, pick and chunk",
            "veiw",
        ],
    );
    // Near is 24 cells: x in 192..2215.
    refuse(
        load(
            &main(&|s| s.replace("local x, y = 1200", "local x, y = 100")),
            &big,
        ),
        &["192..2215", "clamp it"],
    );
    refuse(
        load(
            &main(&|s| {
                s.replace(
                    "w:follow(x, y, h)\nend\nfunction frame",
                    "w:follow(x, y)\nend\nfunction frame",
                )
            }),
            &big,
        ),
        &["w:follow(x, y, angle)"],
    );
    // An angle on a world without a view.
    let m = load(
        &stream_main(4, 20, 600, 600).replace("w:follow(600, 600)", "w:follow(600, 600, 0)"),
        &tmj(200, 300, tile_cell),
    );
    refuse(m, &["takes no angle"]);
    // And the drive itself accepts a view at the catch-up cap's edge: speed 8, ahead 58.
    assert!(engine(
        false,
        &main(&|s| s
            .replace("speed = 4", "speed = 8")
            .replace("ahead = 96, behind = 16", "ahead = 58, behind = 0")),
        &big
    )
    .is_ok());
}

/// A generated world: level1.tmj is six 16x16-cell templates, three across; the ground is 1024 cells round and each
/// chunk shows the template `pick` returns. OPTION is the radius or the view, ANGLE the follow's heading, MOVE a frame.
const GENERATED: &str = r#"
local world = require("@ppu/world")
local plane = dma("track")
seed = 7
picks = 0
local function pick(cx, cy)
  picks += 1
  return (world.hash(seed, cx, cy) + cx * 3 + cy) % 6
end
local w = world.load("level1", {OPTION, period = 1024, pick = PICK})
x, y, h = 8100, 60, 0
function init()
  mode = 7
  w:bind("track", m7)
  w:follow(x, y ANGLE)
end
function frame(t, f)
  MOVE
  w:follow(x, y ANGLE)
end
function probe()
  return {x, y, w.gx, w.gy, picks, w.period, w.templates, w.width, w.height}
end
function hash(s, a, b) return {world.hash(s, a, b)} end
function mhash(s, a, b) return {math.hash(s, a, b)} end
function template(cx, cy) return {pick(cx, cy)} end
function kind(px, py) return {w:tile("track", px, py)} end
"#;

/// Template t's cell (x, y): tileset cells 1..7, so every template differs from its neighbours cell for cell.
fn template_cell(t: usize, x: usize, y: usize) -> u32 {
    1 + (t * 5 + x * 3 + y * 7) as u32 % 7
}

fn templates() -> String {
    tmj(48, 32, |x, y| {
        template_cell(y / 16 * 3 + x / 16, x % 16, y % 16)
    })
}

fn generated(option: &str, angle: &str, movement: &str) -> String {
    GENERATED
        .replace("OPTION", option)
        .replace("ANGLE", angle)
        .replace("MOVE", movement)
        .replace("PICK", "pick")
}

/// Drives a generated world `frames` frames; after each, every cell within `near` cells of the follow point's holds
/// the tile of the template its chunk picks, at the cell's place in the chunk, and `w:tile` answers its kind.
fn fly(main: &str, near: i64, frames: u32) -> Vec<i64> {
    let mut e = engine(false, main, &templates()).unwrap();
    let want = tile_bytes();
    let mut last = vec![];
    for f in 0..frames {
        e.frame(f as f64 / 60.0, f)
            .unwrap_or_else(|e| panic!("frame {f}: {}", e.message));
        last = probe(&mut e);
        let (cx, cy) = (last[0] / 8, last[1] / 8);
        let mut picked = std::collections::HashMap::new();
        for gy in cy - near..cy + near {
            for gx in cx - near..cx + near {
                let (gx, gy) = (gx.rem_euclid(1024), gy.rem_euclid(1024));
                let t = *picked
                    .entry((gx / 16, gy / 16))
                    .or_insert_with(|| e.call_ints("template", &[gx / 16, gy / 16]).unwrap()[0])
                    as usize;
                let cell = template_cell(t, gx as usize % 16, gy as usize % 16);
                let got = e.memory().vram[(gy % 128 * 128 + gx % 128) as usize] & 0xff;
                assert_eq!(got, want(cell), "frame {f}: ground cell ({gx}, {gy}), template {t}, of the point's ({cx}, {cy})");
                if (gx + gy) % 37 == 0 {
                    // Tileset cell 1 is class road (kind 1), 2 wall (kind 2).
                    let kind = e.call_ints("kind", &[gx * 8 + 3, gy * 8 + 5]).unwrap()[0];
                    assert_eq!(
                        kind,
                        if cell <= 2 { i64::from(cell) } else { 0 },
                        "frame {f}: w:tile at cell ({gx}, {gy})"
                    );
                }
            }
        }
    }
    last
}

#[test]
fn a_generated_view_world_shows_each_chunks_picked_template_round_the_period() {
    // From (8100, 60) of 8192 px round: over the seam on both axes and back, turning as it goes.
    let main = generated(
        "view = {ahead = 72, behind = 0}, speed = 4",
        ", h",
        "h = (h + (if f % 200 < 120 then 1 else 254)) % 256\n  x = (x + (if f < 300 then 4 else -3)) % 8192\n  y = (y + (if f < 250 then -2 else 4)) % 8192",
    );
    // The near square (28 cells either side) is exact every frame.
    let p = fly(&main, 28, 600);
    assert_eq!(
        (p[5], p[6], p[7], p[8]),
        (1024, 6, 1024, 1024),
        "period, templates, width, height"
    );
    assert!(
        p[0] < 8100 && p[1] > 60,
        "the point came back over both seams: {p:?}"
    );
}

#[test]
fn a_generated_radius_world_keeps_its_whole_window_and_picks_once_a_chunk() {
    let main = generated("radius = 40", "", "x = (x + (if f < 200 then 4 else -4)) % 8192\n  y = (y + (if f % 90 < 45 then -3 else 4)) % 8192");
    let mut e = engine(false, &main, &templates()).unwrap();
    e.frame(0.0, 0).unwrap();
    // The window is 80 cells a side: 5 or 6 chunks an axis, each picked once (the probe's kinds and templates aside).
    let picks = probe(&mut e)[4];
    assert!((25..=36).contains(&picks), "{picks} picks fill the window");
    let p = fly(&main, 40, 400);
    assert_eq!(
        (p[2], p[3]),
        (p[0] / 8, p[1] / 8),
        "the window's centre is the point's cell"
    );
}

#[test]
fn the_hash_is_the_documented_one_and_the_seed_changes_the_ground() {
    let mut e = engine(false, &generated("radius = 40", "", ""), &templates()).unwrap();
    // world.luau's table, T[i] = q(swap(q(i))) with q(x) = x(2x + 1) mod 256 and swap the nibbles', and its hash.
    let q = |x: i64| x * (2 * x + 1) % 256;
    let t = |i: i64| q(q(i.rem_euclid(256)) % 16 * 16 + q(i.rem_euclid(256)) / 16);
    let hash = |s: i64, x: i64, y: i64| t(t(t(s + x) + y) + t(t(s / 256 + y) + x));
    let mut seen = std::collections::HashSet::new();
    for (s, x, y) in [
        (0, 0, 0),
        (7, 1, 2),
        (1115, 255, 0),
        (9999, 13, 200),
        (65535, 255, 255),
        (300, 40, 41),
    ] {
        assert_eq!(
            e.call_ints("hash", &[s, x, y]).unwrap()[0],
            hash(s, x, y),
            "hash({s}, {x}, {y})"
        );
        // math.hash is the same function; world.hash its deprecated name.
        assert_eq!(
            e.call_ints("mhash", &[s, x, y]).unwrap()[0],
            hash(s, x, y),
            "math.hash({s}, {x}, {y})"
        );
        seen.insert(hash(s, x, y));
    }
    assert!(seen.len() >= 5, "{seen:?}");
    let ground = |seed: i64| {
        let mut e = engine(
            false,
            &generated("radius = 40", "", "").replace("seed = 7", &format!("seed = {seed}")),
            &templates(),
        )
        .unwrap();
        e.frame(0.0, 0).unwrap();
        e.memory().vram[..0x4000]
            .iter()
            .map(|w| *w as u8)
            .collect::<Vec<_>>()
    };
    assert!(ground(7) == ground(7) && ground(7) != ground(8));
}

#[test]
fn what_a_generated_world_cannot_be_is_refused_naming_the_cause() {
    let load = |main: String, map: String| engine(false, &main, &map).err().unwrap_or_default();
    let ok = generated("radius = 40", "", "");
    let m = load(ok.replace("period = 1024", "period = 1000"), templates());
    assert!(
        m.contains("period must be 256, 512, 1024, 2048 or 4096"),
        "{m}"
    );
    let m = load(ok.replace(", period = 1024", ""), templates());
    assert!(m.contains("takes both period") && m.contains("pick"), "{m}");
    let m = load(ok.replace("pick = pick", "pick = 3"), templates());
    assert!(m.contains("takes both period"), "{m}");
    let m = load(ok.replace("radius = 40, ", ""), templates());
    assert!(m.contains("load it with a radius or a view"), "{m}");
    let m = load(ok.clone(), tmj(40, 32, |_, _| 1));
    assert!(
        m.contains("a multiple of 16 cells") && m.contains("40x32"),
        "{m}"
    );
    // A pick that returns no template stops the follow that asks, naming the chunk.
    let m = load(ok.replace("% 6", "% 6 + 6"), templates());
    assert!(m.contains("pick(") && m.contains("0 to 5"), "{m}");
    // The point stays on the ground, wrapped.
    let mut e = engine(
        false,
        &generated("radius = 40", "", "x = x + 4"),
        &templates(),
    )
    .unwrap();
    let m = (0..40)
        .find_map(|f| e.frame(f as f64 / 60.0, f).err())
        .map(|e| e.message)
        .unwrap_or_default();
    assert!(
        m.contains("off the generated ground") && m.contains("x % 8192"),
        "{m}"
    );
}

#[test]
fn a_generated_plane_keeps_its_window_through_moves_of_any_length() {
    // 9 and 24 px a frame (past the window centre's 2-cell step: a whole redraw), and jumps round the seam.
    for movement in [
        "x = (x + 9) % 8192",
        "x = (x + 24) % 8192 y = (y + 16) % 8192",
        "x = (x + 3000) % 8192 y = (y + 8000) % 8192",
    ] {
        fly(&generated("radius = 40", "", movement), 40, 60);
    }
}
