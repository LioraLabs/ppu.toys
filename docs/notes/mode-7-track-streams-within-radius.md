---
id: mode-7-track-streams-within-radius
kind: gotcha
title: A Mode 7 map past 128x128 cells streams around the followed point, and the view must stay inside the loaded radius
codes: []
fixture: racer
---
The plane holds 128x128 cells. A bigger Tiled map loads with `radius` ([[world.load]]): the plane keeps
that many cells on each side of the point you `w:follow`, and loads a row or column as the point crosses
into one. Write the radius as a number. Past `(radius - 1) * 8` pixels from that point the plane shows
cells from elsewhere on the map, so no screen pixel may reach that far at any heading: work out the
farthest corner of your view (the top ground line's ends) and keep it under. Keep the followed point
`radius` cells in from the map's edges, and follow once a frame, in `frame` itself.

```luau
local world = require("@ppu/world")
local _tiles = dma("tiles")
local w = world.load("track", {radius = 64})
local MIN, MAX = 64 * 8, (w.width - 64) * 8
local START = w:find("start")
local x = makeBounded(512, 512, 3584)

function init()
  mode = 7
  screen.main.bg1 = true
  w:bind("road", m7)
  x = math.clamp(START.x, MIN, MAX)
  w:follow(x, START.y)
end

function frame(t, f)
  if pad.right then x = math.min(x + 2, MAX) end
  w:follow(x, START.y)
  m7.cx, m7.cy = x, START.y
  bg[1].scroll.x, bg[1].scroll.y = x - 128, START.y - 113
end
```
