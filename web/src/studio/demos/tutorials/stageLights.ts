/** Tutorial 6/10 — stage-lights: the window system (two hdma-driven spans,
 *  combine logic) composed with colour math (fixed-colour subtract, gated by
 *  the colour window). The heavily commented Lua below IS the tutorial.
 *  Pixels are mirrored byte-for-byte by crates/ppu-core/tests/tutorial_stage_lights.rs. */
import { demo } from "../kit";
import type { Demo, DemoAsset } from "../kit";

const W = 256,
  H = 224;

// 9-colour stage palette (one 4bpp sub-palette holds 15). Channels are
// multiples of 8 so nothing collapses on the rgb15 grid.
const STAGE_PAL: [number, number, number][] = [
  [0, 0, 0], // 0 unused — the asset is fully opaque
  [40, 48, 88], // 1 back-wall fold, dark
  [56, 64, 112], // 2 back-wall fold, lit
  [120, 24, 40], // 3 curtain red, lit
  [88, 16, 32], // 4 curtain red, shadow
  [200, 160, 64], // 5 gold valance trim
  [136, 88, 48], // 6 floorboard, lit
  [112, 72, 40], // 7 floorboard, dark
  [64, 40, 24], // 8 plank seam
  [24, 16, 32], // 9 performer silhouette
];

/** Palette index at (x, y). Integer math only, so the Rust mirror
 *  (stage_index in tutorial_stage_lights.rs) is trivially byte-identical. */
function stageIndex(x: number, y: number): number {
  // The performer, centre stage: head + torso + legs, drawn over the set.
  const hx = x - 128,
    hy = y - 118;
  if (hx * hx + hy * hy <= 100) return 9; // head, r = 10
  if (y >= 128 && y <= 170 && Math.abs(x - 128) <= 8) return 9; // torso
  if (y >= 171 && y <= 204 && (Math.abs(x - 122) <= 3 || Math.abs(x - 134) <= 3)) return 9; // legs
  if (y < 20) return Math.floor(x / 8) % 2 ? 4 : 3; // top valance folds
  if (y < 24) return 5; // gold trim under the valance
  if (y < 176 && (x < 28 || x >= 228)) return Math.floor(x / 8) % 2 ? 4 : 3; // wing curtains
  if (y >= 176) {
    // stage floor: staggered planks with a dark seam every 24px
    const shift = Math.floor((y - 176) / 12) * 12;
    if ((x + shift) % 24 < 2) return 8;
    return Math.floor((x + shift) / 24) % 2 ? 7 : 6;
  }
  return Math.floor(x / 16) % 2 ? 2 : 1; // back-wall drape folds
}

function stage(): DemoAsset {
  const data = new Uint8ClampedArray(W * H * 4);
  for (let y = 0; y < H; y++) {
    for (let x = 0; x < W; x++) {
      const [r, g, b] = STAGE_PAL[stageIndex(x, y)];
      const i = (y * W + x) * 4;
      data[i] = r;
      data[i + 1] = g;
      data[i + 2] = b;
      data[i + 3] = 255;
    }
  }
  return { id: "stage", width: W, height: H, data, kind: "bg", options: { bit_depth: 4 } };
}

// Verbatim-mirrored by MAIN_SRC in crates/ppu-core/tests/tutorial_stage_lights.rs.
const MAIN_SRC = `-- ppu.toys :: stage-lights (6/10 · two hdma spotlights, window combine logic, colour math)
-- Lesson 6 of the tutorial arc (after cavern-camera, before split-screen).
--
-- Three ideas compose here:
--   * A window is a per-scanline SPAN: win.w1/win.w2 are just [lo, hi] columns,
--     so an hdma that rewrites them every scanline can trace any shape.
--   * Combine logic (OR/AND/XOR/XNOR) folds the two windows into ONE region.
--   * Colour math blends each main-screen pixel with an addend (the sub screen
--     or a fixed colour); color.region says WHERE, relative to the colour window.
-- The show: two spotlight irises sweep the stage. Outside the beams the scene is
-- darkened by fixed-colour SUBTRACT (unlike the spotlight demo's clip-to-black,
-- the dark stage stays faintly readable). The windows are XOR-combined, so where
-- the beams cross, the overlap is carved back out of the light.
--
-- Setup stage: one dma copies the stage set into VRAM, once, at compile
-- (parallax-skyline, lesson 2, tells the full story).
local stage = dma("stage", { char = 0x1000, map = 0x0000 })
function frame(t, f)
  apply_pokes()
  mode = 1; brightness = 15
  bg[1].char_base = stage.char; bg[1].map_base = stage.map
  screen.main.bg1 = true    -- the stage, alone on the main screen
  screen.main.bg2 = false; screen.main.bg3 = false   -- power-on defaults ALL layers on: drop the rest
  screen.main.bg4 = false; screen.main.obj = false

  -- The COLOUR window follows BOTH windows. XOR = lit where exactly ONE beam is:
  -- watch the dark lens where the beams cross.
  win.color.w1 = true
  win.color.w2 = true
  win.color.combine = "XOR"

  -- Colour math, gated by the colour window. "outside" = math only where NO beam
  -- lands, so the beams stay untouched and everything else is pulled down.
  color.op = "sub"                  -- main - addend, floored at black
  color.addend = "fixed"            -- addend = the fixed colour, not the sub screen
  color.fixed = rgb(96, 96, 136)    -- blue-heavy subtract: the dark leans warm
  color.on.bg1 = true               -- apply to BG1 pixels (the whole stage)
  color.region = "outside"

  -- Two iris centres. Light 1 hovers near the performer; light 2 sweeps wide.
  local r = 48
  local cx1 = floor(128 + sin(t * 3.1) * 50)
  local cx2 = floor(128 + sin(t * 1.35) * 64)
  local cy1 = floor(134 + sin(t * 0.9) * 8)
  local cy2 = floor(134 - sin(t * 0.9) * 8)
  -- hdma runs this per scanline: each window's span is its circle's chord at
  -- row y (an empty span is lo > hi — that is how a window says "nothing here").
  hdma(0, 223, function(y)
    local d1 = r*r - (y - cy1) * (y - cy1)
    if d1 < 0 then
      win.w1.lo = 1; win.w1.hi = 0
    else
      local hw = floor(sqrt(d1))
      win.w1.lo = cx1 - hw; win.w1.hi = cx1 + hw
    end
    local d2 = r*r - (y - cy2) * (y - cy2)
    if d2 < 0 then
      win.w2.lo = 1; win.w2.hi = 0
    else
      local hw = floor(sqrt(d2))
      win.w2.lo = cx2 - hw; win.w2.hi = cx2 + hw
    end
  end)
  -- Try: combine "OR" (crossed beams merge) or "AND" (only the overlap is lit);
  --      slow light 2 (t * 0.4); tint instead of dim — color.region = "inside",
  --      color.op = "add", color.fixed = rgb(64, 48, 0) warms the beams.
end
`;

export const stageLights: Demo = demo(
  "stage-lights",
  "stage-lights",
  [{ name: "main.lua", source: MAIN_SRC }],
  [stage()],
);
