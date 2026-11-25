---
id: change-cells-with-w-set
kind: recipe
title: Change a map cell in play with w:set, so a path opens or a block breaks
codes: []
fixture: platformer
---
`w:set(layer, x, y, tile)` turns the Tiled tile under map pixel (x, y) into tile id `tile` of the
layer's tileset (Tiled's ID, counting from 0). The change is a real one: `w:tile` answers with the new
tile's Class from then on, and the cell is redrawn at once if it is on screen, or when the camera
streams it in. Lay the tileset out so related tiles sit a fixed distance apart (each closed tile a fixed number
of IDs after its open one), and the code stays a subtraction.

A bind starts the map as authored again, so a scene that re-enters reapplies its changes after
`w:bind` and before the first follow. Read classes as in [[collision-from-tile-classes]].

```luau
local world = require("@ppu/world")
local _storm = dma("storm", {char = 0x1000, pal = 0})
local w = world.load("castle")
local x, y = makeBounded(40, 0, 1023), makeBounded(640, 0, 671) -- the block under the start point

function init()
  mode = 1
  w:bind("ground", bg[2], {map = 0x0000})
  w:follow(0, w.max_y)
  screen.main.bg2 = true
end

function frame(t, f)
  if pad.pressed.a and w:tile("ground", x, y) == w.kinds.solid then
    w:set("ground", x, y, 27) -- the castle wall behind: a broken block
  end
  w:follow(0, w.max_y)
end
```
