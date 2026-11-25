//! A generated world on backgrounds: a sheet of chunk templates, pick(cx, cy) choosing each chunk, streamed into a
//! background as the camera wraps round the period; every sampled frame matches the ground drawn from the sheet image.
mod common;

use common::add_sheet;
use ppu_core::{render_frame, LuaEngine, HEIGHT, WIDTH};
use serde_json::json;

/// A 64x16 px sheet of 16 chars, char 0 blank, each char's pattern its own (one palette).
fn sheet() -> Vec<u8> {
    let mut rgba = vec![0u8; 64 * 16 * 4];
    for c in 1..16usize {
        for py in 0..8 {
            for px in 0..8 {
                let v = (px * 3 + py * 5 + c * 7 + px * py * c) % 5;
                if v == 0 {
                    continue;
                }
                let at = (((c / 8) * 8 + py) * 64 + (c % 8) * 8 + px) * 4;
                let k = ((v + c) % 7) as u8;
                rgba[at..at + 4].copy_from_slice(&[248 - k * 32, k * 32 + 8, 64, 255]);
            }
        }
    }
    rgba
}

/// The template sheet's gids, 1..15 pseudo-random (no empty cells, no flips).
fn gids(n: usize) -> Vec<u32> {
    let mut s: u32 = 0x2468_ace1;
    (0..n)
        .map(|_| {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            1 + s % 15
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
            "tiles": [{"id": 3, "type": "solid"}]
        }],
        "layers": [{"type": "tilelayer", "name": "ground", "width": width, "height": height, "data": data}]
    })
    .to_string()
}

const PERIOD: i64 = 512;
const SIZE: i64 = PERIOD * 8;
/// Camera segments (dx, dy per frame, frames) from near the far corner, across both seams and back.
const PATH: &[(i64, i64, i64)] = &[(8, 7, 120), (-8, 3, 160), (5, -8, 140), (-3, -6, 100)];
const START: (i64, i64) = (SIZE - 500, SIZE - 400);

fn camera(f: i64) -> (i64, i64) {
    let (mut x, mut y, mut left) = (START.0, START.1, f);
    for &(dx, dy, n) in PATH {
        let k = left.min(n);
        x += dx * k;
        y += dy * k;
        left -= k;
    }
    (x.rem_euclid(SIZE), y.rem_euclid(SIZE))
}

/// The pick, as main.luau has it.
fn pick(cx: i64, cy: i64, templates: i64) -> i64 {
    (cx * 7 + cy * 3 + cx * cy) % templates
}

fn main_luau(cw: i64, ch: i64) -> String {
    let segs: Vec<String> = PATH.iter().map(|(dx, dy, n)| format!("{{{dx}, {dy}, {n}}}")).collect();
    format!(
        r#"local world = require("@ppu/world")
local tiles = dma("tiles", {{char = 0x2000, pal = 32}})
local picks = 0
local function pick(cx: number, cy: number): number
  picks += 1
  return (cx * 7 + cy * 3 + cx * cy) % 6
end
local w = world.load("shaft", {{period = {PERIOD}, pick = pick, chunk = {{{cw}, {ch}}}}})
local SIZE = {SIZE}
local PATH = {{{segs}}}
local x, y, seg, left = {sx}, {sy}, 1, PATH[1][3]

function init()
  mode = 1
  brightness = 15
  bg[1].map_base = 0x0800
  w:bind("ground", bg[1])
  w:follow(x, y)
  screen.main.bg1 = true
end

function frame(t, f)
  if f > 0 and seg <= #PATH then
    x, y = (x + PATH[seg][1]) % SIZE, (y + PATH[seg][2]) % SIZE
    left -= 1
    if left == 0 then
      seg += 1
      if seg <= #PATH then left = PATH[seg][3] end
    end
  end
  w:follow(x, y)
end

function probe()
  return {{picks, w:tile("ground", 5, 9)}}
end
"#,
        segs = segs.join(", "),
        sx = START.0,
        sy = START.1,
    )
}

/// The ground's colour at pixel (x, y): its chunk's template's cell, from the sheet.
fn reference(sheet: &[u8], gids: &[u32], (cw, ch, columns): (i64, i64, i64), x: i64, y: i64) -> [u8; 3] {
    let t = pick(x / 8 / cw, y / 8 / ch, 6);
    let (cx, cy) = (t % columns * cw + x / 8 % cw, t / columns * ch + y / 8 % ch);
    let c = gids[(cy * columns * cw + cx) as usize] as usize - 1;
    let at = (((c / 8) * 8 + (y % 8) as usize) * 64 + (c % 8) * 8 + (x % 8) as usize) * 4;
    if sheet[at + 3] == 0 {
        [0; 3]
    } else {
        [sheet[at] >> 3, sheet[at + 1] >> 3, sheet[at + 2] >> 3]
    }
}

fn runs(cw: i64, ch: i64) {
    // Six templates, three across.
    let (columns, rows) = (3, 2);
    let (width, height) = ((columns * cw) as usize, (rows * ch) as usize);
    let (sheet, gids) = (sheet(), gids(width * height));
    let mut e = LuaEngine::new();
    add_sheet(&mut e, "tiles", sheet.clone(), 64, 16, 4);
    let main = main_luau(cw, ch);
    let map = tmj(width, height, &gids);
    e.set_sources(&[("main.luau", main.as_str()), ("shaft.tmj", map.as_str())]).unwrap_or_else(|err| panic!("{}", err.message));
    let total: i64 = PATH.iter().map(|p| p.2).sum();
    let mut checked = 0;
    for f in 0..=total {
        let lt = e.frame(f as f64 / 60.0, f as u32).unwrap_or_else(|err| panic!("frame {f}: {err:?}"));
        if f % 17 != 0 && f != total {
            continue;
        }
        let (cx, cy) = camera(f);
        let fb = render_frame(&lt, e.memory());
        for sy in 0..HEIGHT as i64 {
            for sx in 0..WIDTH as i64 {
                let p = &fb[((sy as usize) * WIDTH + sx as usize) * 4..][..3];
                let got = [p[0] >> 3, p[1] >> 3, p[2] >> 3];
                let want = reference(&sheet, &gids, (cw, ch, columns), (cx + sx) % SIZE, (cy + sy) % SIZE);
                assert_eq!(got, want, "{cw}x{ch}: frame {f}, camera {cx},{cy}: screen pixel {sx},{sy}");
            }
        }
        checked += 1;
    }
    assert!(checked > 20, "only {checked} samples");
    // The path crosses both seams.
    assert!(camera(120).0 < START.0 && camera(120).1 < START.1);
    // w:tile answers from the picked template; the pick ran once per chunk the window took in, not per cell.
    let probe = e.call_ints("probe", &[]).unwrap();
    let picks = probe[0];
    assert!(picks > 0 && picks < 400, "{cw}x{ch}: {picks} picks");
    let t = pick(0, 0, 6);
    let want = gids[((t / columns * ch + 1) * columns * cw + t % columns * cw) as usize];
    assert_eq!(probe[1], if want == 4 { 1 } else { 0 }, "{cw}x{ch}: w:tile");
}

#[test]
fn a_generated_world_on_a_background_streams_its_picked_templates_round_the_period() {
    runs(16, 16);
    runs(32, 16);
    runs(32, 32);
    runs(16, 32);
}
