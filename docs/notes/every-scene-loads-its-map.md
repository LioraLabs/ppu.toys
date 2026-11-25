---
id: every-scene-loads-its-map
kind: concept
title: Every scene that shows a map loads it at file level, and a cartridge keeps one copy of the map
codes: []
fixture: racer
---
A scene switch is a cold boot: the plane and the backgrounds start empty, so a scene that shows a map
loads and binds it itself, even when another scene already did. That costs no ROM: a map named in two
`world.load` calls is stored once. Load at file level of the scene's module, then `bind` and `follow` in
its `init`. In Racer the title, the race and the results each load `track`.

```luau
local world = require("@ppu/world")
local _tiles = dma("tiles")
local w = world.load("track", {radius = 64})
local START = w:find("start")

function init()
  mode = 7
  screen.main.bg1 = true
  w:bind("road", m7)
  w:follow(START.x, START.y)
end

function frame(t, f)
  w:follow(START.x, START.y)
  m7.cx, m7.cy = START.x, START.y
  bg[1].scroll.x, bg[1].scroll.y = START.x - 128, START.y - 113
end
```
