/** parallax-skyline — L1 tutorial toy 2 of 10 (after first-light; mode7-road
 *  is next). Teaches tile-BG image sources, scroll registers, VRAM layout for a
 *  second layer, HDMA scroll bands, and the multi-file shared global scope.
 *
 *  Asset generators are pure + node-safe (raw RGBA) and mirrored byte-for-byte
 *  by crates/ppu-core/tests/tutorial_parallax_skyline.rs — edit both, or the
 *  golden proves nothing about the shipped art.
 *
 *  Both images draw from ONE 14-colour master palette, and both use the whole
 *  set, on purpose. Outside mode 0 every BG source lands its palettes at CGRAM
 *  0 and the SECOND import lands on top — identical colour SETS that fit one
 *  4bpp sub-palette (15 colours; 14 <= 15 is load-bearing) make that overwrite
 *  a no-op. See tilesheet-cavern in demos.ts for the full story. Channels are
 *  multiples of 8 (the rgb15 grid) so none collapse at 5-bit quantization. */
import { demo } from "../kit";
import type { Demo, DemoAsset } from "../kit";

const W = 256, // full screen width: the BG plane wraps a smaller source
  H = 224; // full screen height so the layers do NOT tile vertically
const SPLIT = 168; // near layer's HDMA band boundary — the art changes plane here

const SKYLINE_PAL: [number, number, number][] = [
  [0, 0, 0], //     0 = transparent, never placed
  [8, 8, 24], //    1 sky, zenith
  [24, 16, 56], //  2 sky, mid
  [48, 32, 80], //  3 horizon glow / dark window glass
  [232, 232, 208], // 4 moon / rooftop beacon
  [192, 200, 224], // 5 star / penthouse glint
  [32, 24, 56], //  6 far tower / rooftop tank
  [96, 80, 136], // 7 far lit window
  [16, 8, 24], //   8 foreground silhouette / shaded wall
  [40, 40, 64], //  9 mid building body
  [248, 200, 88], // 10 warm lit window
  [144, 96, 48], // 11 dim lit window
  [56, 48, 80], //  12 roof edge
  [88, 56, 96], //  13 street glow
  [128, 168, 200], // 14 cool (office) window
];

/** Far layer palette index at (x, y) — always opaque: night sky with a moon
 *  and stars over two ranks of distant towers, dissolving into street glow. */
export function farIndex(x: number, y: number): number {
  if (y >= 172) return 13; // city-base glow the towers sink into
  const mx = x - 200,
    my = y - 40;
  if (mx * mx + my * my < 169) {
    // the moon, with a shaded crater
    const cx = x - 196,
      cy = y - 37;
    return cx * cx + cy * cy < 9 ? 5 : 4;
  }
  if (y < 108 && (x * x * 3 + x * 7 + y * y * 5 + y * 3) % 449 === 0) return 5; // stars
  // nearer rank of far towers (visible through the near layer's gaps)
  const b2 = Math.floor(x / 32);
  const h2 = 144 + ((b2 * 23) % 4) * 7;
  if (y >= h2) {
    if (y < h2 + 2) return 12;
    if (x % 32 >= 30) return 8;
    if (x % 8 >= 3 && x % 8 < 5 && y % 8 >= 3 && y % 8 < 5) {
      const w = (Math.floor(x / 8) * 31 + Math.floor(y / 8) * 17) % 7;
      if (w === 0) return 10;
      if (w === 1) return 11;
      if (w === 2) return 14;
    }
    return 9;
  }
  // farthest rank: pure silhouettes with a few pale windows
  const b1 = Math.floor(x / 16);
  const h1 = 112 + ((b1 * 37) % 5) * 7;
  if (y >= h1) {
    if (x % 16 >= 6 && x % 16 < 8 && y % 8 >= 2 && y % 8 < 4) {
      const w = (Math.floor(x / 16) * 13 + Math.floor(y / 8) * 7) % 5;
      if (w === 0) return 7;
      if (w === 1) return 14;
    }
    return 6;
  }
  // sky bands with a checker dither at each seam (8px-periodic -> cheap tiles)
  if (y < 44) return 1;
  if (y < 52) return (x + y) % 2 === 0 ? 1 : 2;
  if (y < 90) return 2;
  if (y < 100) return (x + y) % 2 === 0 ? 2 : 3;
  return 3;
}

/** Near layer palette index at (x, y) — 0 (transparent) wherever the far layer
 *  must show. Rows below SPLIT are a separate foreground plane on purpose: the
 *  HDMA band boundary in skyline.lua sits exactly there, so the two bands never
 *  shear one building apart. */
export function nearIndex(x: number, y: number): number {
  if (y >= SPLIT) {
    // foreground strip: rooftop profile, sparse windows, street glow
    const c = Math.floor(x / 16);
    const hf = SPLIT + ((c * 13) % 3) * 4;
    if (y < hf) return 0;
    if (y >= 216) return 13;
    if (y >= 208) return 3;
    if (y < hf + 1) return 9; // roof lip catching the street light
    if (x % 16 >= 6 && x % 16 < 8 && y % 16 >= 8 && y % 16 < 10) {
      const w = (c * 7 + Math.floor(y / 16) * 5) % 6;
      if (w === 0) return 10;
      if (w === 1) return 11;
      if (w === 2) return 1;
    }
    return 8;
  }
  // mid buildings: a stepped roofline with gap slits the far layer shows through
  const b = Math.floor(x / 32);
  const hm = 92 + ((b * 29) % 5) * 11;
  if (x % 32 >= 26) return 0; // gap between buildings
  if (y < hm) {
    // rooftop furniture above the roofline
    if ((b * 29) % 5 === 0) {
      if (y >= hm - 8 && y < hm - 6 && x % 32 >= 15 && x % 32 < 17) return 4; // beacon
      if (y >= hm - 6 && x % 32 >= 14 && x % 32 < 18) return 6; // its mast
    }
    if ((b * 29) % 5 === 2 && y >= hm - 6 && x % 32 >= 10 && x % 32 < 20) return 6; // water tank
    return 0;
  }
  if (y < hm + 2) return 12; // roof edge
  if (x % 32 >= 24) return 8; // shaded wall
  if (y < hm + 4 && (x + b * 11) % 32 >= 20 && (x + b * 11) % 32 < 22) return 5; // penthouse glint
  if (x % 8 >= 3 && x % 8 < 6 && y % 8 >= 2 && y % 8 < 5) {
    // the window grid: mostly dark glass, a scatter of lit ones
    const w = (Math.floor(x / 8) * 31 + Math.floor(y / 8) * 17) % 9;
    if (w < 2) return 10;
    if (w === 2) return 14;
    if (w === 3) return 11;
    if (w === 4) return 7;
    if (w < 7) return 2;
    return 1;
  }
  return 9;
}

function paint(id: string, indexAt: (x: number, y: number) => number): DemoAsset {
  const data = new Uint8ClampedArray(W * H * 4);
  for (let y = 0; y < H; y++) {
    for (let x = 0; x < W; x++) {
      const idx = indexAt(x, y);
      if (!idx) continue; // palette index 0 -> alpha 0, the layer behind shows
      const [r, g, b] = SKYLINE_PAL[idx];
      const i = (y * W + x) * 4;
      data[i] = r;
      data[i + 1] = g;
      data[i + 2] = b;
      data[i + 3] = 255;
    }
  }
  return { id, width: W, height: H, data, kind: "bg", options: { bit_depth: 4 } };
}

// ── Lua sources (verbatim in crates/ppu-core/tests/tutorial_parallax_skyline.rs) ──
const MAIN_SRC = `-- ppu.toys :: parallax-skyline — tutorial 2 of 10 (after first-light; mode7-road is next)
--
-- A night city in THREE depths from TWO tile layers:
--   bg[2]  far   — stars, moon, distant towers     (slow scroll)
--   bg[1]  near  — mid buildings, above y=168      (faster scroll)
--   bg[1]  near  — foreground strip, below y=168   (fastest — via hdma)
-- The third depth is free: an hdma hook rewrites bg[1]'s scroll per scanline,
-- so ONE layer scrolls at two speeds — the classic SNES parallax-strip trick.
--
-- MULTI-FILE: this toy has two tabs. Chunks run in tab order into ONE shared
-- global scope (PICO-8 style), so FAR_SPEED and band_speed() from skyline.lua
-- are plain globals here. main.lua is a convention, not magic.
function frame(t, f)
  apply_pokes()
  mode = 1; brightness = 15

  -- One image source per layer. In mode 1, bg1 draws over bg2, so the near
  -- image goes on bg1 and is transparent (alpha 0) wherever the sky and far
  -- towers must show through.
  bg[1].source = "skyline_near"
  bg[2].source = "skyline_far"

  -- VRAM is one shared pool, and bg1 already sits at map_base 0 / char_base
  -- 0x1000. A second layer needs its own addresses or the two would overlap:
  bg[2].map_base = 0x0800; bg[2].char_base = 0x4000

  -- Depth = scroll speed. The far layer creeps...
  bg[2].scroll.x = t * FAR_SPEED

  -- ...and the near layer's speed depends on the SCANLINE. Register writes
  -- inside an hdma hook are per-scanline, so this gives bg1 a different scroll
  -- on every row — one layer, several depth bands.
  hdma(0, 223, function(y)
    bg[1].scroll.x = t * band_speed(y)
  end)
end
-- Try: add { y = 208, speed = 140 } to BANDS (the street glow pulls ahead),
--      slow FAR_SPEED to 4 in skyline.lua,
--      or drift the sky: bg[2].scroll.y = sin(t) * 4
`;

const SKYLINE_SRC = `-- parallax-skyline :: skyline.lua — shared constants + the depth-band table.
-- This chunk runs BEFORE frame() is ever called; everything here is a global,
-- visible from every other tab (one shared scope — see main.lua's header).

FAR_SPEED = 12          -- px/sec for the far layer (bg2)

-- The near layer's depth bands: from row .y down to the next entry, scroll at
-- .speed px/sec. The split at 168 matches the art — the image's foreground
-- strip starts on exactly that row, so the seam between bands is invisible.
BANDS = {
  { y = 0,   speed = 40 },   -- mid buildings
  { y = 168, speed = 90 },   -- foreground rooftops + street
}

-- Speed for scanline y: the last band starting at or above y.
function band_speed(y)
  local s = BANDS[1].speed
  for i = 2, #BANDS do
    if y >= BANDS[i].y then s = BANDS[i].speed end
  end
  return s
end
`;

export const parallaxSkyline: Demo = demo(
  "parallax-skyline",
  "parallax-skyline",
  [
    { name: "main.lua", source: MAIN_SRC },
    { name: "skyline.lua", source: SKYLINE_SRC },
  ],
  [paint("skyline_far", farIndex), paint("skyline_near", nearIndex)],
);
