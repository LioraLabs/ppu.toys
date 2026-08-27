/** Tutorial 9/10 :: extbg-direct-color — Mode 7's two deep cuts composed into
 *  one scene: EXTBG (SETINI.6, bit 7 of each floor pixel = per-pixel priority)
 *  and direct colour (CGWSEL.0, the pixel value IS the colour, CGRAM bypassed).
 *  Asset-free on purpose: every byte is poked, and each poked byte is a colour
 *  AND a priority at once. Mirrored byte-for-byte by
 *  crates/ppu-core/tests/tutorial_extbg_direct_color.rs (the golden PNG there
 *  is the frame this file ships) — edit both. */
import { demo } from "../kit";
import type { Demo } from "../kit";

const MAIN_SRC = `-- ppu.toys tutorial 9/10 :: extbg-direct-color — Mode 7's two deep cuts, together
-- (1) EXTBG (SETINI.6): each Mode 7 pixel's bit 7 becomes a per-pixel PRIORITY.
--     The single floor splits into two levels with sprites sandwiched between,
--     front to back:  OBJ3 · M7-high · OBJ2 · OBJ1 · M7-low · OBJ0 · backdrop
--     This is how real carts build pillars a sprite passes BEHIND.
-- (2) Direct colour (CGWSEL.0): the 8-bit pixel value IS the colour — BGR233
--     expanded to BGR555, CGRAM never consulted (see mode7-extbg and
--     direct-color in the demo shelf for each trick alone).
-- Together: EXTBG steals bit 7, so the LOW 7 bits expand instead —
-- 3 bits red, 3 bits green, 1 (dim) bit blue. Every byte poked below is
-- simultaneously a colour and a priority. The pixel values ARE the lesson.
function frame(t, f)
  apply_pokes()
  mode = 7; brightness = 15
  m7.a, m7.d = 1, 1
  m7.extbg = true                        -- bit 7 = priority (SETINI.6)
  direct_color = true                    -- low 7 bits = colour (CGWSEL.0)

  -- Poke tile v solid with value v (tile number == pixel byte, once per value).
  local done = {}
  local function ink(v)
    if not done[v] then
      done[v] = true
      for fy = 0, 7 do for fx = 0, 7 do m7pixel(v, fx, fy, v) end end
    end
    return v
  end

  -- The ground and sky: a smooth two-axis colour field with NO palette. Watch
  -- the packing — this line is the whole direct-colour trick:
  for ty = 0, 27 do
    m7.map[ty] = {}
    for tx = 0, 31 do
      local r, g, b
      if ty < 10 then                    -- dusk sky: blue base, red rising to the horizon
        r, g, b = floor(ty / 3), 0, 1
      else                               -- floor: red follows x, green follows depth
        r = 1 + floor(tx * 6 / 31)
        g = 2 + floor((ty - 10) * 5 / 17)
        b = 0
      end
      local idx = r + g * 8 + b * 64     -- BGR233 packing: r,g in 0..7, b in 0..1 (bit 7 is spoken for)
      m7.map[ty][tx] = ink(idx)          -- bit 7 clear -> LOW floor: sprites float above
    end
  end

  -- The colonnade: same bytes + 0x80. Bit 7 lifts these pixels ABOVE OBJ prio
  -- 2, so the sprite slides behind them — no windows, no second layer.
  local LIT, SHADE, BEAM = 0x80 + 47, 0x80 + 21, 0x80 + 29
  local cols = {4, 15, 26}
  for i = 1, 3 do
    for ty = 3, 15 do
      m7.map[ty][cols[i]] = ink(LIT)     -- sunlit face (r=7 g=5)
      m7.map[ty][cols[i] + 1] = ink(SHADE) -- shaded face (r=5 g=2)
    end
  end
  for tx = 3, 28 do m7.map[2][tx] = ink(BEAM) end -- the lintel across the top

  -- The lantern: a 32x32 sprite at prio 2 — under M7-high, over M7-low.
  -- Sprites still read CGRAM: direct colour is strictly a BG affair.
  cgram[128 + 1] = rgb(255, 220, 80)
  obj.char_base = 0x4000
  obj.size_sel = 1                       -- large pair = 32x32
  for row = 0, 3 do                      -- fill the 4x4 tile block solid (index 1)
    for col = 0, 3 do
      local base = 0x4000 + (row * 16 + col) * 16
      for y = 0, 7 do vram[base + y] = 0x00ff end
    end
  end
  obj[0].tile = 0; obj[0].pal = 0; obj[0].prio = 2
  obj[0].large = true
  obj[0].x = 64 + floor(sin(t * 0.7) * 72)  -- drift through the colonnade...
  obj[0].y = 84 + floor(sin(t * 3) * 4)     -- ...with a gentle bob
  obj[0].on = true
end
-- Try: LIT = 47 (clear its bit 7 — that pillar face drops below the sprite),
-- obj[0].prio = 3 (the lantern rides over everything), or
-- direct_color = false (CGRAM is empty, so the whole floor goes black).
`;

export const extbgDirectColor: Demo = demo(
  "extbg-direct-color",
  "extbg-direct-color",
  [{ name: "main.lua", source: MAIN_SRC }],
  [],
);
