---
id: paths-and-aims-with-lookup
kind: recipe
title: Enemy paths and aimed shots are lookups over (path, phase) and a coarse (dx, dy)
codes: []
---
A [[lookup]] takes several inputs, so one table can hold every closed path an enemy rides: index it by
which path and how far along, and a frame's move is one read. Aiming works the same way: divide the
distance to the target down to a small grid (steps of 32 pixels) and look up a velocity of fixed length.
Clamp the coarse distance to the grid's range, or let the nearest point stand in.

```luau
local PATH = lookup({path = {0, 1}, phase = {0, 255}}, function(path, phase)
  local a = phase * math.pi / 128
  if path == 0 then
    return 128 + math.floor(math.cos(a) * 100), 116 + math.floor(math.sin(a) * 70)
  end
  return 128 + math.floor(math.sin(a) * 104), 116 + math.floor(math.sin(a * 2) * 64)
end)

local AIM = lookup({dx = {-8, 8}, dy = {-7, 7}}, function(dx, dy)
  local length = math.sqrt(dx * dx + dy * dy)
  if length == 0 then return 0, 1.5 end
  return dx / length * 1.5, dy / length * 1.5
end)

local phase = makeBounded(0, 0, 255)
local ex = makeBounded(128, 0, 255)
local ey = makeBounded(116, 0, 223)
local vx = makeFixed(0, -2, 2, 16)
local vy = makeFixed(0, -2, 2, 16)

function frame(t, f)
  phase = (phase + 1) % 256
  ex, ey = PATH(1, phase)
  vx, vy = AIM((128 - ex) // 32, (200 - ey) // 32)
end
```
