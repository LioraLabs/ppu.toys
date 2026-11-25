---
id: w-set-redraws-at-most-16-columns-an-update
kind: gotcha
title: w:set redraws at most 16 columns of on-screen cells between follows, and refuses layers that wrap, stream or sit on Mode 7
codes: []
fixture: platformer
---
[[change-cells-with-w-set|`w:set`]] changes the whole Tiled tile, so a 16-pixel tile is two cell columns. An update may redraw at most 16 columns of on-screen cells between follows (a cartridge queues them for the next vertical blank beside
the follow's strips): spread a bigger change, a whole screen of coins turning into sky, over several updates. Changes to cells off screen have no limit and draw when the camera uncovers them. Only sheet layers drawn on backgrounds change: a layer on the Mode 7 plane, a layer
bound with `wrap`, and a world loaded with `radius`, `view` or `period` refuse `w:set`. On a cartridge a layer a `w:set` names keeps a copy of its tiles in work RAM, 2 bytes a Tiled tile, taking what room bank $7F has left; past it the build is refused,
naming each layer and the cells that would fit, so change only the layers that need it.

```luau
local world = require("@ppu/world")
local _tiles = dma("tiles", {palette = "tiles_pal", char = 0x1000, pal = 32})
local w = world.load("meadow")
local SKY = 0 -- the tileset's empty tile
local cleared = makeBounded(0, 0, 8)

function init()
  mode = 1
  w:bind("ground", bg[2], {map = 0x0000})
  w:follow(0, 0)
  screen.main.bg2 = true
end

function frame(t, f)
  w:follow(0, 0)
  -- one tile a frame, never a whole screen of them in one update
  if f % 8 == 0 and cleared < 8 then
    w:set("ground", 16 * cleared, 32, SKY)
    cleared = cleared + 1
  end
end
```
