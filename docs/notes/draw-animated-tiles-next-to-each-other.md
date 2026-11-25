---
id: draw-animated-tiles-next-to-each-other
kind: gotcha
title: Animated tiles side by side in the tileset image with the same frame durations change in one copy, so lay them out together
codes: []
fixture: platformer
---
Each frame change copies the tile's pixels (32 bytes per 8x8 cell at 4 bits a pixel) during vertical blank on a cartridge, a copy for each run of tiles that change together. Animated tiles that sit side by side in the tileset image, with the same frame durations, change in one copy, and layers that share a tileset
share its copies. So draw a tileset's animated tiles next to each other (water, then its frames, then lava), and give the tiles that should blink together the same durations. How to set an animation up
is in [[animated-tiles-are-set-up-in-tiled]].

```luau
local world = require("@ppu/world")
local _tiles = dma("tiles", {palette = "tiles_pal", char = 0x1000, pal = 32})
local w = world.load("meadow")

function init()
  mode = 1
  w:bind("ground", bg[2], {map = 0x0000})
  w:follow(0, 0)
  screen.main.bg2 = true
end

function frame(t, f)
  w:follow(0, 0)
end
```
