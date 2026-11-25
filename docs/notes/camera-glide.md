---
id: camera-glide
kind: recipe
title: A gliding camera moves an eighth of the way to its target each frame, at most 8 pixels
codes: []
fixture: platformer
---
To ease the camera after a target (a walker on a map, a path that just opened far away), move it an
eighth of the remaining distance each frame, at least one pixel so it settles, and at most 8 pixels.
Eight pixels is one row or column of cells, so a follow never streams more than that in a frame, and
a long way back stays cheap: the camera simply takes longer to arrive.

Clamp the target to the map first ([[clamp-the-camera]]), then glide each axis, then
[[follow-once-a-frame|follow once]].

```luau
local world = require("@ppu/world")
local _storm = dma("storm", {char = 0x1000, pal = 0})
local w = world.load("castle")
local camx, camy = makeBounded(0, 0, 768), makeBounded(0, 0, 448) -- MAP SIZE: castle.tmj is 1024x672 px
local x, y = makeBounded(400, 0, 1023), makeBounded(300, 0, 671)

local function sign(v: number): number return if v > 0 then 1 elseif v < 0 then -1 else 0 end

local function glide(c: number, aim: number, edge: number): number
  local d = math.clamp(aim, 0, edge) - c
  return c + sign(d) * math.clamp(math.abs(d) // 8, 1, 8)
end

function init()
  mode = 1
  w:bind("ground", bg[2], {map = 0x0000})
  w:follow(0, 0)
  screen.main.bg2 = true
end

function frame(t, f)
  camx, camy = glide(camx, x - 128, w.max_x), glide(camy, y - 112, w.max_y)
  w:follow(camx, camy)
end
```
