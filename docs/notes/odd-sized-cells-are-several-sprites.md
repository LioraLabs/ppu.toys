---
id: odd-sized-cells-are-several-sprites
kind: concept
title: A cell that is not a hardware size is drawn as several sprites, and every draw, key, flip and group counts them all
codes: []
fixture: twinstar
---
Art takes the size of its sheet's cells, 8 to 128 pixels in steps of 8. A cell the sprite hardware has no size for (24x24, or 64x32 with the 8 and 32 pair)
is built from the pair's sprites: its large ones in a grid from the cell's corner, then its small ones beside and below.
A 24x24 cell with the 8 and 16 pair is one 16x16 and five 8x8. Sprites whose tiles are all blank are left out, and each
sprite is clipped alone, so one that is off screen takes no slot. The pieces move as one unit: a [[keyed-draws-and-keep|key]],
a keep, a flip and a group slot count all of them, and each piece counts toward the [[sprite-line-limits-drop-the-top-group-first|line limits]].
Cut cells at a hardware size when a sprite must be one piece. On a cartridge a draw costs about half a percent of a frame for each sprite cell it shows, so count cells, not actors, when a screen gets busy.

```luau
local sheet = dma("ships", {palette = "ships_pal", char = 0x4000, pal = 0})
-- Two side by side 16x16 cells draw as one 32x16 art: two sprites in one draw.
local WIDE = sprites.art({{size = 16, columns = 2, cells = {0, 1}}}, sheet)
local ships = sprites.group()

function init()
  mode = 1
  screen.main.obj = true
  obj.char_base = sheet.char
end

function frame(t, f)
  ships:draw(WIDE, 128, 104)
end
```
