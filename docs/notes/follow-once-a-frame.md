---
id: follow-once-a-frame
kind: rule
title: Follow each world exactly once a frame, from frame() itself
codes: []
fixture: platformer
---
`w:follow(x, y)` moves the camera and streams the cells the move uncovers into the background, so a
cartridge wants it called once every frame, in `frame()` itself (or the scene's frame), after the
camera has been worked out: not in a helper some frames skip, not twice. Work out the camera first,
in a function that returns numbers, then follow; one follow streams every layer the world bound.

Keep each step small: a camera that jumps far in one frame redraws much more than a row and a
column (see [[camera-glide|a gliding camera]]). A deliberate jump, a door to another place on the
map, is fine under a fade ([[doors-from-map-objects|doors]]). Set up the world as in
[[tiled-maps-for-big-levels]].

```luau
local world = require("@ppu/world")
local _tiles = dma("tiles", {palette = "tiles_pal", char = 0x1000, pal = 32})
local w = world.load("meadow")
local x = makeBounded(40, 0, 1535)
local cam = makeBounded(0, 0, 1280)

function init()
  mode = 1
  w:bind("ground", bg[2], {map = 0x0000})
  w:follow(0, 0)
  screen.main.bg2 = true
end

function frame(t, f)
  if pad.right then x = math.min(x + 2, 1535) end
  if pad.left then x = math.max(x - 2, 0) end
  cam = math.clamp(x - 128, 0, w.max_x)
  w:follow(cam, 0) -- once, here, after the camera is known
end
```
