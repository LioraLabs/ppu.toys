---
id: move-one-axis-then-snap
kind: recipe
title: Move one axis at a time against the tiles, then snap flush against the 16 px tile in the way
codes: []
fixture: platformer
---
Move x by the speed, ask the [[collision-from-tile-classes|tiles]] whether the body now overlaps a
solid one, and if it does, put it back flush against that tile and zero the speed; then the same for
y. One axis at a time is what lets a body slide along a wall instead of sticking to it. Snapping to
the tile's edge (`x // 16 * 16` plus the body's half width) leaves no gap and no overlap, whatever the
speed was.

The snap is plain arithmetic on positions, so it lives in the shared hero module and the scene that
owns the map asks the tiles ([[world-calls-stay-in-the-scene|world calls stay in the scene]]). Clamp
the position to the map too: a [[makeFixed]] position must stay inside its bounds.

```luau
local world = require("@ppu/world")
local _tiles = dma("tiles", {palette = "tiles_pal", char = 0x1000, pal = 32})
local w = world.load("meadow")
local x, vx = makeFixed(40, 0, 1535, 8), makeFixed(0, -3, 3, 8)

local function solid(px: number, py: number): boolean
  return w:tile("ground", px, py) == w.kinds.solid
end

-- Where a 14 px wide body moved to nx, stopped by a tile beside it, stands flush against it.
local function snap_x(nx: number, dir: number): number
  local ix = math.floor(nx)
  return if dir > 0 then (ix + 7) // 16 * 16 - 8 else (ix - 7) // 16 * 16 + 23
end

function frame(t, f)
  vx = if pad.right then 2 elseif pad.left then -2 else 0
  local nx = math.clamp(x + vx, 8, 1527)
  local ix = math.floor(nx)
  if (vx > 0 and solid(ix + 7, 170)) or (vx < 0 and solid(ix - 7, 170)) then
    nx, vx = snap_x(nx, vx), 0
  end
  x = nx
end
```
