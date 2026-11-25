---
id: world-calls-stay-in-the-scene
kind: rule
title: A world's calls stay in the scene that shows it; shared modules take positions, never the world
codes: []
fixture: platformer
---
Load, bind, follow, `w:tile` and `w:set` belong to the scene file that shows the map. Code several
scenes share (the hero, enemies, a HUD line, a camera) takes numbers: the camera's top left, a
position, a speed, and returns numbers or booleans; the scene asks its own map and passes the answer
in. The templates do this throughout: the platformer's levels each move the hero against their own
tiles and call `Player.snap_x` to put him back, and the adventure's `camera.luau` works out the camera
that `play.luau` follows.

A module that keeps a world handle, or asks a map for its tiles, ties itself to one scene's map and
breaks the next scene that uses it. See [[move-one-axis-then-snap|moving against tiles]] for the
shared snap functions.

```luau
local world = require("@ppu/world")
local _tiles = dma("tiles", {palette = "tiles_pal", char = 0x1000, pal = 32})
local w = world.load("meadow")
local cam = makeBounded(0, 0, 1280)

-- Shared code (a camera module) takes positions and returns numbers: it works for any map.
local function camera_for(x: number, max_x: number): number
  return math.clamp(x - 128, 0, max_x)
end

function init()
  mode = 1
  w:bind("ground", bg[2], {map = 0x0000})
  w:follow(0, 0)
  screen.main.bg2 = true
end

function frame(t, f)
  cam = camera_for(f % 1536, w.max_x)
  w:follow(cam, 0) -- the scene owns the world
end
```
