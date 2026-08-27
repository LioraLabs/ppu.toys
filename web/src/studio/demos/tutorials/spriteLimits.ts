/** Tutorial 10/10 :: sprite-limits — the per-scanline OBJ budgets (32 sprites,
 *  34 tile slivers), the STAT77 overflow flags, and the classic flicker
 *  mitigation (rotating obj.first). Asset-free: solid tiles poked via vram[].
 *  The Lua is mirrored byte-for-byte by
 *  crates/ppu-core/tests/tutorial_sprite_limits.rs (the golden PNG there is
 *  the frame this file ships) — edit both. */
import { demo } from "../kit";
import type { Demo } from "../kit";

const MAIN_SRC = `-- ppu.toys tutorial 10/10 :: sprite-limits — why 90s games flickered
-- The PPU re-evaluates all 128 OAM sprites on EVERY scanline, under two
-- hard budgets:
--   RANGE: only the first 32 sprites whose Y covers the line are kept
--   TIME:  the kept sprites may fetch at most 34 8px-wide tile slivers
-- Anything past a budget simply does not draw on that line. Both events
-- latch into STAT77 ($213E) — open the Sprites inspector and watch the
-- RANGE OVER and TIME OVER badges light up while this frame renders.
-- The classic fix was never "use fewer sprites": rotate where evaluation
-- STARTS (obj.first) each frame, so a DIFFERENT set pays the budget every
-- frame — flicker, the fair alternative to permanent invisibility.
function frame(t, f)
  apply_pokes()
  mode = 1; brightness = 15
  local ROTATE = true   -- flip to false: the SAME sprites vanish forever
  obj.char_base = 0x4000
  obj.size_sel = 1      -- OBSEL pair 1: small = 8x8, large = 32x32
  -- solid 4bpp tiles 0..63 (colour index 1) so every sprite is a filled square
  for tn = 0, 63 do
    local base = 0x4000 + tn * 16
    for y = 0, 7 do vram[base + y] = 0x00ff end
  end
  cgram[0] = rgb(16, 16, 32)                 -- backdrop: dark navy
  cgram[128 + 1]      = rgb(80, 220, 100)    -- pal 0: control row, green
  cgram[128 + 16 + 1] = rgb(255, 168, 40)    -- pal 1: dense band, amber
  cgram[128 + 32 + 1] = rgb(255, 232, 140)   -- pal 2: dense band, pale amber
  cgram[128 + 48 + 1] = rgb(90, 180, 255)    -- pal 3: big-sprite row, blue

  -- ROW A, sprites 0..7 — the CONTROL. Eight 8x8 sprites on one line: under
  -- both budgets, so this row never loses a sprite. Compare the rows below
  -- against it.
  for i = 0, 7 do
    obj[i].tile = 0; obj[i].pal = 0
    obj[i].x = 16 + i * 30; obj[i].y = 32
    obj[i].on = true
  end

  -- ROW B, sprites 8..47 — the RANGE overflow. Forty 8x8 sprites all share
  -- scanlines 96..103. 40 > 32, so on every one of those lines the last 8 in
  -- evaluation order are dropped: the band comes up short of the right edge.
  -- Columns alternate two shades so you can count exactly which survive.
  for i = 0, 39 do
    local s = 8 + i
    obj[s].tile = 0; obj[s].pal = 1 + i % 2
    obj[s].x = 4 + i * 6; obj[s].y = 96
    obj[s].on = true
  end

  -- ROW C, sprites 48..57 — the TIME overflow, a different budget entirely.
  -- Only ten sprites, nowhere near the 32 cap — but each large 32x32 sprite
  -- spans 4 tile slivers per line, so 10 * 4 = 40 slivers > 34. The PPU keeps
  -- the evaluation-order prefix that fits (8 sprites = 32 slivers; a 9th
  -- would need 36) and the last two big squares vanish with the sprite
  -- budget barely touched. Big sprites are cheap in OAM, expensive in fetch.
  for i = 0, 9 do
    local s = 48 + i
    obj[s].tile = 0; obj[s].pal = 3
    obj[s].large = true
    obj[s].x = 4 + i * 24; obj[s].y = 150
    obj[s].on = true
  end

  -- The mitigation. obj.first sets where OAM evaluation starts (OAMADD
  -- priority rotation); the budgets still bite, but a different run of
  -- sprites pays each frame. With ROTATE = false the gaps freeze: the same
  -- sprites are invisible on every frame, forever. With true the gaps crawl
  -- through the rows at 60Hz — flicker shares the pain.
  if ROTATE then obj.first = f % 58 else obj.first = 0 end
end
-- Try: flip ROTATE and watch the gaps freeze; widen ROW B's loop to 47 and
-- more columns die; set large = true in ROW B too — 40 * 4 = 160 slivers
-- slams the 34-tile budget on top of the 32-sprite cap.
`;

export const spriteLimits: Demo = demo(
  "sprite-limits",
  "sprite-limits",
  [{ name: "main.lua", source: MAIN_SRC }],
  [],
);
