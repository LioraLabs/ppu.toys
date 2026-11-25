---
id: w-enter-cuts-to-another-map-under-force-blank
kind: recipe
title: w:enter(name, x, y) is a hard cut to another loaded map, drawn whole, with the screen off for the update that makes it
codes: []
fixture: platformer
---
A world shows one map at a time on its backgrounds. Load each level's map at file level (in the scene or program that shows it), then `w:enter(name, x, y)` cuts to
map `name` with the camera at `(x, y)`: every bound layer is redrawn whole at once. The screen must be off, so set `force_blank = true` before the call and turn
it off on a later update; fade with `brightness` first if you like, the game chooses the transition. `name` is a literal, an entry of a constant list, or a Tiled
door's property (`w:enter(door.props.to, x, y)`), and `w:enter(w.name, x, y)` jumps within the map shown, which is also how a level restarts.
Enter at most once per world an update, where a follow could stand. See [[w-enter-is-refused-when-the-maps-do-not-match]]
for what it refuses, and [[doors-from-map-objects]] for the doors that call it. On a cartridge the redraw keeps the screen black for about 4 to 5 pictures with two bound backgrounds, so cover it with a fade or a door animation at least that long.

```luau
local world = require("@ppu/world")
local _tiles = dma("tiles", {palette = "tiles_pal", char = 0x1000, pal = 32})
local w = world.load("meadow")
local held = false

function init()
  mode = 1
  w:bind("ground", bg[2], {map = 0x0000})
  w:follow(0, 0)
  screen.main.bg2 = true
end

function frame(t, f)
  force_blank = false -- the update after a cut turns the screen back on
  w:follow(0, 0)
  if pad.a and not held then
    force_blank = true
    w:enter(w.name, 0, 0)
  end
  held = pad.a
end
```
