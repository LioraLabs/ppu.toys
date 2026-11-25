---
id: collision-from-tile-classes
kind: recipe
title: Collision comes from each tile's Class in Tiled, read with w:tile and compared with w.kinds
codes: []
fixture: platformer
---
Give tiles a Class in the Tiled tileset (`solid`, `ledge`, `hazard`, `water`, `road`); a tile with no
class is scenery. `w:tile(layer, x, y)` returns the class number of the tile under map pixel (x, y),
0 off the map, and `w.kinds` maps each class name to its number, so compare them:
`w:tile("ground", x, y) == w.kinds.solid`. The same test is road against grass in a racer and wall
against floor in an adventure.

Ask a few points of the body, not every pixel: a platformer hero asks three points up his side for a
wall and two under his feet for the floor. Then [[move-one-axis-then-snap|move one axis at a time]].
Load the map as in [[tiled-maps-for-big-levels]].

```luau
local world = require("@ppu/world")
local _tiles = dma("tiles", {palette = "tiles_pal", char = 0x1000, pal = 32})
local w = world.load("meadow")
local x, y = makeBounded(40, 0, 1535), makeBounded(176, 0, 223)

-- Whether a wall stands at x beside feet at y: three points up a 24 px body.
local function wall(px: number, py: number): boolean
  return w:tile("ground", px, py - 2) == w.kinds.solid
    or w:tile("ground", px, py - 12) == w.kinds.solid
    or w:tile("ground", px, py - 22) == w.kinds.solid
end

function frame(t, f)
  if pad.right and not wall(x + 8, y) then x = math.min(x + 1, 1535) end
  if w:tile("ground", x, y - 6) == w.kinds.hazard then x = 40 end
end
```
