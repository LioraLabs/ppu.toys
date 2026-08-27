/** Tutorial 1/10 :: first-light — what a register is, the backdrop (cgram[0]),
 *  brightness, and a first hdma() per-scanline hook. Asset-free on purpose:
 *  the whole sunrise comes out of registers alone. The Lua is mirrored
 *  byte-for-byte by crates/ppu-core/tests/tutorial_first_light.rs (the golden
 *  PNG there is the frame this file ships) — edit both. */
import { demo } from "../kit";
import type { Demo } from "../kit";

const MAIN_SRC = `-- ppu.toys tutorial 1/10 :: first-light — registers, the backdrop, brightness, hdma
-- A register is a little memory slot on the picture chip: write a number into
-- it and the picture changes. That is all driving a PPU ever is. This whole
-- sunrise is three registers — no images anywhere:
--   cgram[0]     the backdrop colour: what shows where nothing else drew
--   brightness   the master fade, 0 (black) .. 15 (full) - INIDISP
--   color.fixed  the fixed colour (COLDATA), here ADDed onto the backdrop
-- frame(t, f) runs every frame: t = seconds, f = frame number. Edits re-run
-- live, and apply_pokes() replays your inspector tweaks (see pokes.lua).
function frame(t, f)
  apply_pokes()
  brightness = 15
  cgram[0] = hsl(268 + sin(t / 7) * 14, 0.55, 0.24)  -- pre-dawn violet, drifting

  -- Colour math: ADD a fixed colour onto the backdrop, every pixel.
  color.op = "add"; color.addend = "fixed"; color.on.backdrop = true

  -- hdma(y0, y1, fn) runs fn once per scanline y, top to bottom, after
  -- frame() returns. A register write inside lands on that line ONLY — one
  -- backdrop colour fans out into 224. This is the classic SNES gradient.
  local horizon = 150
  hdma(0, 223, function(y)
    if y < horizon then
      local a = y / horizon                        -- 0 up top .. 1 at the horizon
      color.fixed = rgb(a * a * 250, a * a * a * 165, a * 50)  -- dawn glow builds down the sky
      brightness = 11 + floor(a * 4)               -- night still hangs at the top
      if y >= horizon - 4 then brightness = 15 end -- first light: the sun's edge
    else
      local d = (y - horizon) / 73                 -- 0 at the horizon .. 1 at the bottom
      local g = (1 - d) * (1 - d) * (0.7 + 0.3 * (y % 2))    -- odd lines shimmer
      color.fixed = rgb(g * 220, g * 100, g * 45 + 15)       -- the sky reflects off the water
      brightness = 9 - floor(d * 4)                -- and the water fades into the dark
    end
  end)
end
-- Try: move horizon, swap "add" for "sub", or speed up the sky: sin(t / 7) -> sin(t)
`;

export const firstLight: Demo = demo(
  "first-light",
  "first-light",
  [{ name: "main.lua", source: MAIN_SRC }],
  [],
);
