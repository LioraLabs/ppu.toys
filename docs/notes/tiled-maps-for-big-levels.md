---
id: tiled-maps-for-big-levels
kind: recipe
title: A level bigger than the screen is a Tiled map loaded at file level, bound in init, and drawn whole by the first follow
codes: []
fixture: platformer
---
Draw the level in Tiled as `<name>.tmj` beside the code, from a tileset whose PNG is an image source,
and [[world.load|`world.load("<name>")`]] it from [[@ppu/world]] at file level, once. In `init`, bind each
tile layer to its own background with `w:bind(layer, bg[n], {map = ...})` (the background map's VRAM
address, clear of the tiles), then call `w:follow(x, y)` with the camera's top left: the first follow
after a bind draws the whole screen, and every follow after it draws only the rows and columns the
move uncovered. A bind starts the map as authored again, so a scene that restarts binds again.

Never fill a big map with `vram[]` loops: a console shows black for seconds. `w.width` and
`w.height` count 8x8 cells, so a map of 16 px tiles is `w.width * 8` pixels wide. Then
[[follow-once-a-frame|follow it once a frame]] and [[clamp-the-camera|clamp the camera]].

```luau
local world = require("@ppu/world")
local _tiles = dma("tiles", {palette = "tiles_pal", char = 0x1000, pal = 32})
local w = world.load("meadow")
local START = w:find("start")
local cam = makeBounded(0, 0, 1280)

function init()
  mode = 1
  w:bind("ground", bg[2], {map = 0x0000})
  cam = math.clamp(START.x - 128, 0, w.max_x)
  w:follow(cam, 0) -- the first follow after a bind draws the whole screen
  screen.main.bg2 = true
end

function frame(t, f)
  w:follow(cam, 0)
end
```
