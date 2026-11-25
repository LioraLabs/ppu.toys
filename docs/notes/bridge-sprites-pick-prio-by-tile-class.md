---
id: bridge-sprites-pick-prio-by-tile-class
kind: recipe
title: Over an EXTBG plane a sprite at prio 1 passes under the high pixels and prio 3 over them, chosen by the class of the tile it stands on
codes: []
fixture: twinstar
---
With an [[mode-7-extbg-priority-mask|EXTBG mask]] on the plane (and `screen.main.bg2` on, since BG2 draws the high pixels), a sprite drawn at `prio = 1` passes under the high pixels and over the low ones, and at `prio = 3` it is over everything: use
those two. Let the map decide which, with Tiled classes read through [[collision-from-tile-classes|w:tile]]: a car that comes up a tile classed `ramp` is on the bridge (3), and one that comes along the road below is under it (1). Remember the last class
it crossed in a flag, because the tile under the car is the same road on both levels.

```luau
local sheet = dma("ships", {palette = "ships_pal", char = 0x4000, pal = 0})
local CAR = sprites.art({{size = 16, columns = 1, cells = {0}}}, sheet)
local racers = sprites.group()
local up = false

function init()
  screen.main.obj = true
  obj.char_base = sheet.char
end

function frame(t, f)
  -- with a map: local kind = w:tile("road", x, y); up = true on w.kinds.ramp, false on w.kinds.road
  up = f % 240 < 120
  racers:draw(CAR, 128, 160, {prio = if up then 3 else 1})
end
```
