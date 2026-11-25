---
id: one-way-ledges
kind: recipe
title: A one-way ledge holds the feet only when they were above its top before the move
codes: []
fixture: platformer
---
Class the ledge tiles `ledge` in Tiled. While falling, a solid tile under the feet always lands them;
a ledge lands them only when the feet were at or above the tile's top on the frame before. Rising
through a ledge from below, or walking past one, never touches it. Snap the feet to the tile's top
(`fy // 16 * 16`) when they land, and never test ledges while rising.

This is the falling half of [[move-one-axis-then-snap|moving one axis at a time]], with the classes
from [[collision-from-tile-classes]].

```luau
local world = require("@ppu/world")
local _tiles = dma("tiles", {palette = "tiles_pal", char = 0x1000, pal = 32})
local w = world.load("meadow")
local x = 120
local y, vy = makeFixed(100, 0, 256, 8), makeFixed(0, -6, 6, 8)

function frame(t, f)
  vy = math.min(5, vy + 0.25)
  local ny = y + vy
  local fy = math.floor(ny)
  local top = fy // 16 * 16
  local under = w:tile("ground", x, fy)
  if vy >= 0 and (under == w.kinds.solid or (under == w.kinds.ledge and y <= top)) then
    y, vy = top, 0
  else
    y = math.clamp(ny, 0, 256)
  end
end
```
