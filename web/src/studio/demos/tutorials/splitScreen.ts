/** Tutorial toy 7/10 :: split-screen — per-scanline `mode` override: a mode 1
 *  city band up top, a mode 7 perspective floor below, in ONE frame. The Lua
 *  and the city() pixels are mirrored byte-for-byte by
 *  crates/ppu-core/tests/tutorial_split_screen.rs (the golden PNG proves the
 *  frame the studio ships).
 *
 *  Only ONE import ships on purpose: the engine binds sources once per frame
 *  using the frame-wide mode (place_bg_sources in ppu-core/src/lua.rs), so a
 *  mode 7 texture import cannot bind in this mode 1 frame — the floor is poked
 *  into mode 7 tile 0 from Lua instead. The Lua header teaches exactly that.
 */
import { demo } from "../kit";
import type { Demo, DemoAsset } from "../kit";

const W = 256;
const H = 224;
const SPLIT = 112; // rows 0..111 = mode 1 city, 112..223 = mode 7 floor

// ── the city: dusk skyline, drawn only in the rows where mode 1 shows ────────
// 9 colours total (one 4bpp sub-palette holds 15), every channel a multiple of
// 8 so nothing collapses on the rgb15 grid. Integer math only — the Rust
// mirror must produce identical bytes.
const SKY_BANDS: Array<[number, number, number, number]> = [
  [20, 24, 16, 64], // y < 20: deep indigo
  [44, 56, 24, 88],
  [68, 104, 40, 96],
  [88, 168, 64, 88],
  [SPLIT, 224, 104, 72], // warm right above the split
];

function city(): DemoAsset {
  const data = new Uint8ClampedArray(W * H * 4);
  for (let y = 0; y < SPLIT; y++) {
    for (let x = 0; x < W; x++) {
      let r = 0,
        g = 0,
        b = 0;
      for (const [limit, br, bg, bb] of SKY_BANDS) {
        if (y < limit) {
          r = br;
          g = bg;
          b = bb;
          break;
        }
      }
      // low sun, partly hidden behind the far skyline
      const dx = x - 200,
        dy = y - 56;
      if (dx * dx + dy * dy < 196) {
        r = 248;
        g = 224;
        b = 152;
      }
      // far skyline (lighter silhouette), 3px gaps so the dusk glow leaks through
      const farTop = 58 + ((Math.floor(x / 16) * 13) % 5) * 6;
      if (x % 16 < 13 && y >= farTop) {
        r = 72;
        g = 48;
        b = 104;
      }
      // near buildings (dark), 2px gaps between blocks
      const gap = x % 24 >= 22;
      const nearTop = 68 + ((Math.floor(x / 24) * 37) % 33);
      if (!gap && y >= nearTop) {
        r = 16;
        g = 16;
        b = 32;
        // lit windows: 2x2 cells on an 6x8 grid, about half of them on
        const wx = x % 24;
        if (
          y >= nearTop + 3 &&
          y < 108 &&
          wx % 6 >= 2 &&
          wx % 6 <= 3 &&
          y % 8 >= 2 &&
          y % 8 <= 3 &&
          (Math.floor(x / 6) * 5 + Math.floor(y / 8) * 3) % 4 < 2
        ) {
          r = 248;
          g = 208;
          b = 88;
        }
      }
      const i = (y * W + x) * 4;
      data[i] = r;
      data[i + 1] = g;
      data[i + 2] = b;
      data[i + 3] = 255;
    }
    // rows >= SPLIT stay zeroed: transparent, they never show in mode 1
  }
  return { id: "city", width: W, height: H, data, kind: "bg", options: { bit_depth: 4 } };
}

// ── the Lua IS the tutorial ──────────────────────────────────────────────────
const MAIN_SRC = `-- ppu.toys tutorial 7/10 :: split-screen -- two hardware modes in ONE frame
--
-- A background MODE is the PPU's whole personality: how many layers you get
-- and how many colours each one has. Mode 1 is the workhorse tile mode; mode 7
-- is the single affine plane every racing game steered. The chip reads the
-- mode from one register -- BGMODE ($2105) -- and it reads it EVERY scanline.
-- Real SNES games split the frame by pointing an HDMA channel at BGMODE: tile
-- scenery up top, a perspective floor below. Setting that up on hardware took
-- a DMA channel and a table in WRAM. Here it is one line: assign 'mode' INSIDE
-- an hdma hook and only those scanlines flip. A per-scanline mode switch is a
-- trick only this DSL does.
--
-- One catch, and it is this toy's second lesson: imports bind ONCE per frame,
-- using the frame-wide mode. Mode 1 is frame-wide here, so the city binds as a
-- normal 4bpp BG import (parallax-skyline taught those) -- but a mode 7
-- texture import can NOT bind in a mode 1 frame, so the floor is poked into
-- mode 7 tile 0 instead (extbg-direct-color pokes the plane the same way).
-- The two worlds still share VRAM without colliding: mode 7 owns words
-- 0x0000-0x3FFF, so the city's tiles are parked above at 0x4000.
function frame(t, f)
  apply_pokes()
  mode = 1; brightness = 15    -- frame-wide default: every line starts in mode 1

  -- Top band: the city (rows 0..111 of the import; the floor covers the rest).
  bg[1].source = "city"
  bg[1].char_base = 0x4000     -- clear of the mode 7 words (0x0000-0x3FFF)
  bg[1].map_base = 0x7000
  screen.main.bg1 = true
  screen.main.bg2 = false; screen.main.bg3 = false  -- power-on defaults ALL layers
  screen.main.bg4 = false; screen.main.obj = false  -- on; they'd show garbage here

  -- Bottom band's texture: ONE 8x8 tile. The zeroed mode 7 map reads tile 0 at
  -- every cell, so whatever tile 0 holds repeats across the whole 1024x1024
  -- plane -- an endless floor from 64 pixels.
  cgram[0] = rgb(16, 8, 40)          -- backdrop, should anything miss
  cgram[16] = rgb(56, 40, 72)        -- asphalt A   (16..18 sit clear of the
  cgram[17] = rgb(248, 168, 248)     -- grid line     city's palette in 0..15:
  cgram[18] = rgb(40, 28, 56)        -- asphalt B    CGRAM is shared by both bands)
  for py = 0, 7 do
    for px = 0, 7 do
      local c = 16
      if (floor(px / 4) + floor(py / 4)) % 2 == 1 then c = 18 end  -- 4px checker
      if px == 0 or py == 0 then c = 17 end                        -- neon grid seams
      m7pixel(0, px, py, c)
    end
  end

  -- The split. The hook runs once per scanline y; every register assignment
  -- inside lands on that line ONLY.
  local split = 112
  hdma(split, 223, function(y)
    mode = 7                          -- THE line: mode 7 for scanlines 112..223
    local d = 64 / (y - (split - 1))  -- perspective divide: far rows step the
    m7.a, m7.d = d, d                 -- texture faster, near rows slower (mode7-road)
    m7.cx, m7.cy = 128, 0             -- vanishing point sits on the split
    bg[1].scroll.y = t * 80 * d       -- drive forward; scaling by d keeps depth honest
  end)
end
-- Try: move the split with 112 + floor(sin(t) * 16) (both uses of 'split' follow);
-- steer with m7.cx = 128 + sin(t) * 40; or set the frame-wide mode to 3 and watch
-- the city vanish -- an import only binds where its depth matches the mode's.
`;

export const splitScreen: Demo = demo(
  "split-screen",
  "split-screen",
  [{ name: "main.lua", source: MAIN_SRC }],
  [city()],
);
