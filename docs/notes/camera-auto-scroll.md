---
id: camera-auto-scroll
kind: recipe
title: An auto-scrolling camera moves on its own at a set pace, and the hero loses when it leaves him behind
codes: []
fixture: platformer
---
For a chase or a rising shaft, the camera moves by itself: a [[makeFixed]] value that steps by a
fraction of a pixel a frame (0.5 in the platformer's climb), after a second's grace at the start. It
also moves faster when the hero gets near the leading edge, so he always sees what is ahead, and it
stops at the map's end. A hero whose feet fall off the trailing edge of the screen loses a life.

Follow the whole-pixel part, `math.floor(cam)`, and stop the camera with the map's literal bounds
([[clamp-the-camera]]).

```luau
local world = require("@ppu/world")
local _tiles = dma("glimmer", {palette = "glimmer_pal", char = 0x1000, pal = 32})
local w = world.load("climb")
local RISE = 0.5
local cam = makeFixed(1120, 0, 1120, 4) -- MAP SIZE: the climb's height less one screen
local y = makeBounded(1300, 0, 1343)

function init()
  mode = 1
  w:bind("ground", bg[2], {map = 0x0000})
  cam = w.max_y
  w:follow(0, math.floor(cam))
  screen.main.bg2 = true
end

function frame(t, f)
  if pad.up then y = math.max(y - 2, 0) end
  if f >= 60 then cam = math.max(0, math.min(cam - RISE, y - 88)) end
  w:follow(0, math.floor(cam))
  if y > cam + 232 then y = 1300 end -- left behind
end
```
