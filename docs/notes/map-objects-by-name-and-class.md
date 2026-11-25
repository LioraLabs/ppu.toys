---
id: map-objects-by-name-and-class
kind: recipe
title: Place things on a Tiled object layer; find one by name with w:find and read the rest by Class once, at file level
codes: []
fixture: platformer
---
Put the start point, enemies, pickups and goals on an object layer in Tiled instead of in code.
`w:find("start")` returns the one object with that name (an error when there is none or more than
one), and `w.objects` lists every object with its `name`, `type` (its Tiled Class), `x`, `y`, `w`, `h`
in map pixels and its custom `props`. Read them once at file level into your own records, so a frame
only walks small tables and a restart resets the records rather than reading the map again.

Count each list as it fills, in a [[makeBounded]] count, and write by index: the count bounds the list,
so the cartridge drops the index check ([[snes.unproven_index]]), and a map with more stops at power-on.
Each record's moving values get their own [[makeBounded]] bounds there. Load the map as in
[[tiled-maps-for-big-levels]]; doors are objects too ([[doors-from-map-objects|doors]]).

```luau
local world = require("@ppu/world")
local w = world.load("meadow")
local START = w:find("start")
local shards = {}
local nshards = makeBounded(0, 0, 32)
local goal = {x = 0, y = 0}
for _, o in w.objects do
  if o.type == "pickup" then
    nshards += 1
    shards[nshards] = {x = o.x + 8, y = o.y + 8, taken = false}
  elseif o.type == "goal" then
    goal.x, goal.y = o.x + o.w // 2, o.y + o.h - 8
  end
end
local x = makeBounded(START.x, 0, 1535)

function frame(t, f)
  for _, s in shards do
    if math.abs(s.x - x) < 12 then s.taken = true end
  end
end
```
