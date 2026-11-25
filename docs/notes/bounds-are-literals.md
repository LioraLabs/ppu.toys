---
id: bounds-are-literals
kind: gotcha
title: makeBounded and makeFixed bounds are integer literals, so a bound taken from the map is written as a number and marked
codes: []
---
The minimum and maximum of [[makeBounded]] and [[makeFixed]] must be integer literals: `makeBounded(0, 0,
MAX_X)` is refused, even when `MAX_X` is a constant (see [[snes.bounded]]). The clamp in the code may use the
constant; the contract repeats the number.

When the bound comes from a map's size, pick one of two ways. Write the map's current size and tag every
such line `-- MAP SIZE`, so growing the map means finding those lines (Adventure, Platformer). Or size the
bound for the largest map the game allows, so the map can grow in Tiled without touching the code (Racer:
512x512 cells). Either way the clamp keeps the value inside it.

```luau
local MAX_X = 1792 -- MAP SIZE: the map's width less one screen
local cam = {x = makeBounded(0, 0, 1792)} -- MAP SIZE

function frame(t, f)
  local dx = (if pad.right then 2 else 0) - (if pad.left then 2 else 0)
  cam.x = math.clamp(cam.x + dx, 0, MAX_X)
end
```
