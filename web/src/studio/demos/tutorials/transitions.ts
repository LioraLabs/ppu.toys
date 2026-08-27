/** Tutorial toy 8/10 :: transitions — brightness fades, mosaic fades,
 *  force blank, and the hdma brightness wipe, cycling over one scene.
 *  Lua + pixel generator mirrored byte-for-byte by
 *  crates/ppu-core/tests/tutorial_transitions.rs (the golden proves the frame
 *  the studio ships). */
import { demo } from "../kit";
import type { Demo, DemoAsset } from "../kit";

const W = 256,
  H = 224;
const HORIZON = 140; // sky/ridges above, water below

/** |((x % p) - p/2)| — integer triangle wave (identical in the Rust mirror). */
function tri(x: number, p: number): number {
  return Math.abs((x % p) - p / 2);
}

/** Dusk vista: gradient sky, square sun, two ridge lines, flecked water.
 *  Deliberately full of sub-8px detail (1px ridge checker, 2px water flecks)
 *  so the mosaic phases visibly flatten it — while every pattern is 8px- or
 *  band-periodic, keeping unique tiles and 8bpp colours cheap. Integer math
 *  only, so the Rust mirror is byte-identical. */
function scene(): DemoAsset {
  const data = new Uint8ClampedArray(W * H * 4);
  for (let y = 0; y < H; y++) {
    for (let x = 0; x < W; x++) {
      const i = (y * W + x) * 4;
      let r: number, g: number, b: number;
      if (y >= HORIZON) {
        // water: darkens with depth; bright 2px flecks drift per row
        const db = Math.floor((y - HORIZON) / 4);
        const fleck = (x + y * 5) % 8 < 2;
        r = 144 - db * 4 + (fleck ? 64 : 0);
        g = 72 - db * 2 + (fleck ? 56 : 0);
        b = 56 + db * 2 + (fleck ? 32 : 0);
      } else {
        const r1 = 70 + tri(x + 16, 64); // far ridge top: 70..102
        const r2 = 104 + Math.floor(tri(x + 96, 128) / 2); // near ridge top: 104..136
        if (y >= r2) {
          // near ridge: dark with a 1px checker shimmer
          const c = (x + y) % 2 === 0 ? 1 : 0;
          r = 48 + c * 10;
          g = 32 + c * 8;
          b = 72 + c * 12;
        } else if (y >= r1) {
          r = 94; // far ridge, flat violet
          g = 62;
          b = 118;
        } else if (x >= 176 && x < 200 && y >= 40 && y < 64) {
          r = 255; // the sun, an 8px-aligned square
          g = 224;
          b = 152;
        } else {
          // dusk sky gradient, stepped every 4 rows
          const band = Math.floor(y / 4);
          r = 32 + band * 4;
          g = 28 + band * 2;
          b = 96 - band * 2;
        }
      }
      data[i] = r;
      data[i + 1] = g;
      data[i + 2] = b;
      data[i + 3] = 255;
    }
  }
  return { id: "vista", width: W, height: H, data, kind: "bg", options: { bit_depth: 8 } };
}

const MAIN_SRC = `-- ppu.toys tutorial 8/10 :: transitions — the scene-change toolkit built into the chip
-- Every SNES fade, flash, and level-exit is one of three registers plus a trick:
--   brightness    INIDISP bits 0-3 — the master fade, 0 (black) .. 15 (full)
--   mosaic        MOSAIC $2106 — block size 0..15; bg[n].mosaic is the same
--                 register's per-layer enable bit
--   force_blank   INIDISP bit 7 — the hard cut: the frame goes black NOW,
--                 whatever brightness says
--   the trick     write brightness inside hdma() and the fade lands per
--                 SCANLINE — a wipe down the screen no whole-frame fade can do
--
-- One scene, one 12-second loop, every transition in turn:
--   0-2    wipe-in      hdma brightness front sweeps down, revealing the scene
--   2-4    hold         the scene, untouched
--   4-6    fade         brightness 15 -> 0 -> 15
--   6-8    mosaic       block size 0 -> 15 -> 0: the picture chunks apart
--   8-10   mosaic+fade  both at once — the classic SNES level-exit
--   10-12  hold         ...cut by a force_blank blink at 11s; the loop's
--                       wipe-in reopens from that black
local CYCLE = 12

-- Setup stage: one dma copies the 8bpp scene into VRAM at compile — the
-- loading screen (parallax-skyline, lesson 2, tells the full story).
local vista = dma("vista", { char = 0x1000, map = 0x0000 })

function frame(t, f)
  apply_pokes()
  mode = 3                       -- 8bpp BG1: one full-colour scene to transition over
  bg[1].char_base = vista.char
  bg[1].map_base = vista.map
  bg[1].mosaic = true            -- enabling is free: size 0 below means "off"
  brightness = 15                -- the holds ARE these two defaults;
  mosaic = 0                     -- each phase only overrides what it needs

  local tc = t % CYCLE           -- t grows forever: schedule off the remainder
  local phase = floor(tc / 2)    -- six 2-second phases
  local u = tc - phase * 2       -- 0..2 inside the current phase

  if phase == 0 then
    -- WIPE-IN. brightness written inside hdma() lands on ONE scanline, so a
    -- moving front with a short ramp reads as a curtain lifting down the frame.
    local edge = u * 144         -- the front's scanline, sweeping past 223
    hdma(0, 223, function(y)
      brightness = min(15, max(0, floor((edge - y) / 2)))  -- full 30px above the front, black below
    end)
  elseif phase == 2 then
    -- FADE. One register, out and back: 15 -> 0 -> 15 across the two seconds.
    brightness = floor(15 * abs(u - 1))
  elseif phase == 3 then
    -- MOSAIC. The same triangle on the block size; bg[1].mosaic opted BG1 in.
    mosaic = floor(15 * (1 - abs(u - 1)))
  elseif phase == 4 then
    -- BOTH. Chunk apart while dimming — the level-exit half the SNES library used.
    local k = 1 - abs(u - 1)
    mosaic = floor(15 * k)
    brightness = 15 - floor(11 * k)   -- dim to 4, not 0: keep the chunks readable
  elseif phase == 5 and u >= 1 and u < 1.15 then
    -- THE CUT. force_blank blanks the frame no matter what brightness says.
    force_blank = true
  end
  -- (phases 1 and 5 otherwise fall through: the hold is the defaults above)
end
-- Try: retime the schedule (CYCLE and the /2), steepen the wipe (the /2 in the
-- hdma), let the level-exit fade all the way out, or blank the whole last phase.
`;

export const transitions: Demo = demo(
  "transitions",
  "transitions",
  [{ name: "main.lua", source: MAIN_SRC }],
  [scene()],
);
