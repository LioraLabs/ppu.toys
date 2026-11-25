---
id: animated-tiles-are-set-up-in-tiled
kind: recipe
title: Animated tiles (water, lava, torches) are given frames in Tiled, and every copy of the tile on every bound layer plays with no code
codes: []
fixture: platformer
---
Open the tileset in Tiled, select a tile, and give it frames in the Tile Animation Editor: each frame is a tile of the same tileset and a duration. Once a layer is bound and
followed, every cell that draws the tile plays the animation wherever it is on the map. It works as SNES games did it: when a frame changes, the tile's own pixels in VRAM are overwritten with
the frame's during vertical blank, so every copy changes at once and no map cell is written. Durations round to video frames, at least one each, and the clock is the frame counter `f` (counted
from the scene's start in a program of scenes), so the Studio and a cartridge show the same frame. Every copy of a tile shows the same frame:
for torches that flicker out of step, draw two tiles with the same frames in a different order. A frame keeps the tile's palette: draw the frames in the tile's own colours or the map is refused. Only background tilesets
animate; a Mode 7 tileset's tiles are shared across the plane and animating one is refused. After `w:enter`, the entered map's layer plays its own animations.

```luau
local world = require("@ppu/world")
local _tiles = dma("tiles", {palette = "tiles_pal", char = 0x1000, pal = 32})
local w = world.load("meadow")
local x = makeBounded(0, 0, 1280)

function init()
  mode = 1
  w:bind("ground", bg[2], {map = 0x0000})
  w:follow(0, 0)
  screen.main.bg2 = true
end

function frame(t, f)
  x = math.min(1280, x + 1) -- the animated tiles in the cells that scroll in play without any call
  w:follow(x, 0)
end
```
