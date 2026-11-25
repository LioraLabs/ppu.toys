---
id: camera-dead-zone
kind: recipe
title: A dead-zone camera moves only when the hero leaves a box round the middle of the screen
codes: []
fixture: adventure
---
Keep a box round the screen's middle (64 by 48 pixels in the adventure). While the hero's feet are
inside it the camera holds still; when they cross an edge, the camera moves just enough to put that
edge back under them. Small steps and turns do not scroll the screen, and a run scrolls it at the
hero's speed. Clamp each result to the map ([[clamp-the-camera]]).

Work the camera out from positions in its own function or module and pass the result to the scene's
one [[follow-once-a-frame|follow]].

```luau
local world = require("@ppu/world")
local _tiles = dma("tiles", {palette = "tiles_pal", char = 0x1000, pal = 32})
local w = world.load("overworld")
local BOX_W, BOX_H = 64, 48
local camx, camy = makeBounded(0, 0, 1792), makeBounded(0, 0, 1792) -- MAP SIZE
local x, y = makeBounded(128, 0, 2047), makeBounded(112, 0, 2015) -- MAP SIZE

local function update()
  local left, top = 128 - BOX_W // 2, 112 - BOX_H // 2
  local sx, sy = x - camx, y - camy
  if sx < left then camx = math.max(0, x - left)
  elseif sx > left + BOX_W then camx = math.min(w.max_x, x - left - BOX_W) end
  if sy < top then camy = math.max(0, y - top)
  elseif sy > top + BOX_H then camy = math.min(w.max_y, y - top - BOX_H) end
end

function init()
  mode = 1
  w:bind("ground", bg[2], {map = 0x0000})
  w:follow(0, 0)
  screen.main.bg2 = true
end

function frame(t, f)
  if pad.right then x = math.min(x + 1, 2047) end
  if pad.down then y = math.min(y + 1, 2015) end
  update()
  w:follow(camx, camy)
end
```
