---
id: sprite-line-limits-drop-the-top-group-first
kind: gotcha
title: A scanline shows 32 sprites and 34 eight-pixel slivers, and the sprites that draw on top lose slivers first
codes: []
fixture: twinstar
---
The hardware shows at most 32 sprites on a scanline and loads at most 34 eight-pixel slivers of them (a 32-pixel-wide
sprite is four slivers on every line it covers). A line past 34 loses slivers of its first sprites, the ones that draw
on top, so the [[sprite-groups-stack-in-creation-order|group made first]] is the one that goes. A line past 32 loses
whole sprites, the last ones. So make the group that must never lose a sliver (the player) after every
group that can spare one. To spread the loss across a crowd instead,
set [[sprites.flicker]]: each unsorted group rotates its unkeyed sprites every frame, so a line with more than 32
sprites flickers rather than always dropping the same ones. Keyed sprites are never rotated.

```luau
local sheet = dma("ships", {palette = "ships_pal", char = 0x4000, pal = 0})
local DRONE = sprites.art({{size = 16, columns = 1, cells = {6}}}, sheet)
local SHIP = sprites.art({{size = 16, columns = 1, cells = {0}}}, sheet)
-- Drones first (they lose slivers first), the player's group after them.
local drones = sprites.group({slots = 12})
local ships = sprites.group()

function init()
  mode = 1
  screen.main.obj = true
  obj.char_base = sheet.char
  sprites.flicker = true
end

function frame(t, f)
  for i = 0, 11 do
    drones:draw(DRONE, 16 + i * 18, 100)
  end
  ships:draw(SHIP, 128, 104)
end
```
