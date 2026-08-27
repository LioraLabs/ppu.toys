/** Tutorial toy 7/10 :: split-screen — per-scanline `mode` override: a mode 1
 *  city band up top, a mode 7 perspective floor below, in ONE frame. The Lua
 *  and the city()/floorTex() pixels are mirrored byte-for-byte by
 *  crates/ppu-core/tests/tutorial_split_screen.rs (the golden PNG proves the
 *  frame the studio ships).
 *
 *  TWO imports, placed by dma(): the city as tile data at explicit addresses,
 *  the floor as a real m7 payload in the interleaved 0x0000 region — dma
 *  places by the payload's committed kind, not the frame-wide mode, which is
 *  what lets both worlds coexist in one frame.
 *
 *  CGRAM is the one thing the two bands genuinely share. The floor is drawn
 *  from THREE colours the city already owns — by construction they are the
 *  city's three lowest BGR555 palette entries AND they sort into the same
 *  order under the m7 importer's [r,g,b] palette sort, so both placements
 *  write the same values into CGRAM 1..3 and neither band's colours break.
 *  tutorial_split_screen.rs pins that invariant against the live importers. */
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

// ── the floor: a 1024x1024 mode 7 texture, three colours borrowed from the city ──
// A neon grid over checkered asphalt, sliding under the perspective divide.
// Every period (32px grid, 8px checker) divides 1024, so the plane tiles
// seamlessly; the pattern repeats every 32px, so the whole plane dedups to 16
// unique 8x8 tiles. The three colours are EXACTLY the city's three lowest
// BGR555 palette entries (see the header): navy asphalt = the near buildings,
// indigo checker = the top sky band, warm grid = the band above the split —
// so the shared CGRAM 1..3 entries agree between the two dma placements.
export function floorTex(): DemoAsset {
  const w = 1024,
    h = 1024;
  const data = new Uint8ClampedArray(w * h * 4);
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      let r, g, b;
      if (x % 32 < 2 || y % 32 < 2) {
        [r, g, b] = [224, 104, 72]; // sunset grid seams
      } else if ((Math.floor(x / 8) + Math.floor(y / 8)) % 2 === 1) {
        [r, g, b] = [24, 16, 64]; // asphalt B (indigo)
      } else {
        [r, g, b] = [16, 16, 32]; // asphalt A (navy)
      }
      const i = (y * w + x) * 4;
      data[i] = r;
      data[i + 1] = g;
      data[i + 2] = b;
      data[i + 3] = 255;
    }
  }
  return { id: "floor", width: w, height: h, data, kind: "m7", options: {} };
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
-- Setup stage: BOTH worlds go into VRAM up front, each by its payload's own
-- rules (parallax-skyline, lesson 2, tells the full dma story). The city is
-- tile data at addresses we pick; a mode 7 texture always lands interleaved
-- at 0x0000-0x3FFF (that region is hard-wired into the chip), which is
-- exactly why the city's tiles park above it at 0x4000. dma places by what
-- the data IS, not by what mode the frame runs in -- that is the whole
-- reason one frame can hold both worlds.
local city = dma("city", { char = 0x4000, map = 0x7000 })
dma("floor")   -- m7: chars+map at 0x0000, its 3-colour palette at CGRAM 1..
-- One palette serves both bands: CGRAM has no notion of the split. The floor
-- is deliberately painted with three colours the city already owns, so its
-- palette lands on entries 1..3 holding the exact same values -- shared
-- paint instead of a fight.
function frame(t, f)
  apply_pokes()
  mode = 1; brightness = 15    -- frame-wide default: every line starts in mode 1

  -- Top band: point BG1 at the city's VRAM home from the setup stage.
  bg[1].char_base = city.char  -- clear of the mode 7 words (0x0000-0x3FFF)
  bg[1].map_base = city.map
  screen.main.bg1 = true
  screen.main.bg2 = false; screen.main.bg3 = false  -- power-on defaults ALL layers
  screen.main.bg4 = false; screen.main.obj = false  -- on; they'd show garbage here

  cgram[0] = rgb(16, 8, 40)    -- backdrop, should anything miss

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
-- steer with m7.cx = 128 + sin(t) * 40; or swap the floor for mode7-road's
-- ground: drag any image into assets as an m7 texture and change the name in
-- dma("floor").
`;

export const splitScreen: Demo = demo(
  "split-screen",
  "split-screen",
  [{ name: "main.lua", source: MAIN_SRC }],
  [city(), floorTex()],
);
