---
id: tiled-objects-are-rectangles-and-points-and-a-layer-has-one-tileset
kind: gotcha
title: A map's objects must be unrotated rectangles or points, and a tile layer must draw from one tileset, or the map is refused
codes: []
fixture: platformer
---
`w.objects` lists the rectangles and points on a map's object layers, in layer order, each `{name, type, x, y, w, h, props}` with `x` and `y` in map pixels and `props` holding the custom properties. Tile, polygon, polyline, text and template objects are refused, and so is a rotated object:
set its rotation to 0 in Tiled. A tile layer draws from one tileset; a layer that uses tiles from two is refused with a note to split it into two layers, each bound to its own background. Read the objects at file level or in `init()` and make your own records from them
([[map-placed-actors-are-record-lists]]); `w.objects` stays as it was loaded until `w:enter` shows another map. Name the ones the code needs and find them by name ([[map-objects-by-name-and-class]]).

```luau
local world = require("@ppu/world")
local _tiles = dma("tiles", {palette = "tiles_pal", char = 0x1000, pal = 32})
local w = world.load("meadow")
local START = w:find("start") -- a rectangle or point named in Tiled
local spawn = {x = START.x, y = START.y}
local px = makeBounded(0, 0, 1536)

function frame(t, f)
  px = math.min(1536, spawn.x + f)
end
```
