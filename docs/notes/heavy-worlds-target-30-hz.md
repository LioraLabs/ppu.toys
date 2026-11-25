---
id: heavy-worlds-target-30-hz
kind: recipe
title: A world with several streaming layers runs at 60 Hz and slows down when busy; targetHz 30 is the author's choice, not a fix
codes: []
fixture: platformer
---
Each bound layer costs a cartridge part of every update that streams its row and its column, and a Mode 7 map that streams (a radius or a view) costs more the larger the radius. The cartridge build's frame budget
reports it with the rest of the game. A game that misses its frame on busy updates keeps 60 Hz and the SNES slows down,
as SNES games did; an author who wants a steady pace instead sets `"targetHz": 30` in `ppu.json` (or the Studio setting), so an update lasts two video frames. Do not drop layers to chase the budget before reading its note: [[snes.budget]] names what costs most.

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
  x = math.min(1280, x + 4) -- at 30 Hz this is 2 px a video frame
  w:follow(x, 0)
end
```
