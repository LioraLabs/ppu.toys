---
id: w-enter-is-refused-when-the-maps-do-not-match
kind: gotcha
title: w:enter is refused, changing nothing, when the screen is on, the map is not loaded, or the maps cannot share the world's backgrounds
codes: []
fixture: platformer
---
A refused enter names the cause and leaves the world as it was. It is refused when `force_blank` is false; when the scene (or the program, for a program world)
loads no map of that name, so load it at file level there; when the world is on the Mode 7 plane or loaded with `radius`, `view` or `period`; when the map lacks a layer the
world binds, or draws it from a sheet placed at a different `char` or `pal`; when the map's background map does not fit at the bound `map` (0x800 words when it is wider than 32 cells,
0x400 otherwise) or would overlap a placement; and when the camera `(x, y)` leaves the new map. Levels that change tileset, as
the platformer template's realms do, are therefore separate scenes, each placing its own tileset at the same addresses, rather than `w:enter`s ([[placements-a-scene-owns]]).

```luau
local world = require("@ppu/world")
local _tiles = dma("tiles", {palette = "tiles_pal", char = 0x1000, pal = 32})
local w = world.load("meadow")
local LAST = w.max_x

function init()
  mode = 1
  w:bind("ground", bg[2], {map = 0x0000})
  w:follow(0, 0)
  screen.main.bg2 = true
end

function frame(t, f)
  force_blank = false
  w:follow(0, 0)
  if pad.pressed.b then
    force_blank = true
    -- the camera must lie on the map: 0 to max_x, never past it
    w:enter(w.name, math.min(LAST, 400), 0)
  end
end
```
