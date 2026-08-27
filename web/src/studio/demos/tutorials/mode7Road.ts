/** Tutorial 3/10 :: mode7-road — the namesake: Mode 7's affine matrix, the
 *  per-scanline perspective divide, and a ground texture that is just an image
 *  source the reader can replace with their own drag-dropped photo. The Lua and
 *  the road() pixels are mirrored byte/pixel-identical by
 *  crates/ppu-core/tests/tutorial_mode7_road.rs (the golden PNG there is the
 *  frame this file ships) — edit both. */
import { demo } from "../kit";
import type { Demo, DemoAsset } from "../kit";

/** The ground plane: grey asphalt with a dashed yellow centreline and white
 *  edge lines, dirt shoulders, mottled grass beyond. The road strip is centred
 *  on texel column 128 because the Lua pins m7.cx there. Every period (dash 32,
 *  mottles 8/16) divides 1024, so the plane tiles seamlessly under wrap. */
export function road(): DemoAsset {
  const w = 1024,
    h = 1024;
  const data = new Uint8ClampedArray(w * h * 4);
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const i = (y * w + x) * 4;
      let r, g, b;
      if (x >= 124 && x < 132 && y % 32 < 18) {
        [r, g, b] = [240, 208, 48]; // dashed yellow centreline
      } else if ((x >= 34 && x < 40) || (x >= 216 && x < 222)) {
        [r, g, b] = [232, 232, 232]; // solid white edge lines
      } else if (x >= 28 && x < 228) {
        const m = ((Math.floor(x / 8) + Math.floor(y / 8)) % 2) * 6;
        [r, g, b] = [88 + m, 88 + m, 94 + m]; // asphalt, faintly mottled
      } else if (x >= 16 && x < 240) {
        [r, g, b] = [146, 108, 66]; // dirt shoulders
      } else {
        const m = (Math.floor(x / 16) + Math.floor(y / 16)) % 2;
        [r, g, b] = [40 + m * 8, 116 + m * 16, 40 + m * 8]; // grass
      }
      data[i] = r;
      data[i + 1] = g;
      data[i + 2] = b;
      data[i + 3] = 255;
    }
  }
  return { id: "road", width: w, height: h, data, kind: "m7", options: {} };
}

const MAIN_SRC = `-- ppu.toys tutorial 3/10 :: mode7-road — the affine matrix, the perspective
-- divide, and a ground plane you can drop your own photo onto.
--
-- Mode 7 is the SNES's one big "3D" trick, and it is nothing but sampling: BG1
-- becomes a single 1024x1024 image read through an affine transform. Four
-- matrix registers do all the work:
--   m7.a m7.d   texels stepped per screen pixel (the scale)
--   m7.b m7.c   the cross terms (rotation / shear — 0 here, so no turn)
--   m7.cx m7.cy the pivot the transform scales around
-- Held constant, that only zooms and spins a flat picture. The "3D" floor is a
-- per-scanline lie: an hdma hook rewrites the scale on EVERY line below the
-- horizon, dividing by how far down the screen the line sits. Far lines take
-- big steps across the image (so it draws small), near lines tiny ones. One
-- divide per line, and a flat image lies down into a road.
--
-- THE PART TO STEAL: "road" is only an image source. Drag ANY png onto the
-- assets panel and point bg[1].source at its name — your photo becomes the
-- ground plane. That drag-drop is the whole reason this site exists.
function frame(t, f)
  apply_pokes()
  mode = 7; brightness = 15        -- mode 7: BG1 is now the affine layer
  bg[1].source = "road"            -- the ground image — swap in your own here

  local HORIZON = 96               -- screen row where ground meets sky
  local SCALE   = 128              -- eye height, in effect: bigger = higher up
  local SPEED   = 160              -- driving speed, ground texels per second

  -- Sky. Above the horizon nothing should draw, so the backdrop (cgram[0])
  -- shows through: park the plane out of reach up there — wrap mode 2 makes
  -- off-plane samples transparent — then haze the backdrop toward the horizon
  -- with the fixed-colour gradient trick from first-light (tutorial 1).
  cgram[0] = rgb(64, 120, 208)     -- backdrop = deep sky blue
  m7.wrap = 2                      -- off the 1024x1024 plane -> transparent
  bg[1].scroll.y = -4096           -- and above the horizon we ARE off it
  color.op = "add"; color.addend = "fixed"; color.on.backdrop = true
  hdma(0, HORIZON, function(y)
    local a = y / HORIZON                       -- 0 at the top .. 1 at the horizon
    color.fixed = rgb(a * 90, a * 70, a * 40)   -- warm haze builds downward
  end)

  -- The floor. Below the horizon, rebuild the camera on every single scanline.
  hdma(HORIZON + 1, 223, function(y)
    local d = SCALE / (y - HORIZON)  -- THE perspective divide: scale ~ 1/distance
    m7.a, m7.d = d, d                -- d texels per pixel, both axes (b, c stay 0)
    m7.cx, m7.cy = 128, 0            -- pin screen centre to texel column 128 —
                                     -- exactly where the texture paints its road
    m7.wrap = 0                      -- down here the plane tiles forever...
    bg[1].scroll.y = (t * SPEED) / d -- ...and slides by at a constant texel speed
                                     -- at every depth: /d undoes the row's own
                                     -- scale. That IS the driving.
  end)
  -- Near the horizon the tiling shows: ghost copies of the road converge on
  -- the vanishing point. Every SNES racer had this — most hid it under fog.
end
-- Try: move HORIZON, or SCALE = 64 to hug the ground
-- Try: a bend — in the floor hook add  bg[1].scroll.x = (223 - y) * (223 - y) / 600
-- Try: the namesake — drag a photo into assets, set bg[1].source = "its-name"
`;

export const mode7Road: Demo = demo(
  "mode7-road",
  "mode7-road",
  [{ name: "main.lua", source: MAIN_SRC }],
  [road()],
);
