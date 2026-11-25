---
id: map-placed-actors-are-record-lists
kind: recipe
title: Actors placed on a Tiled map are a plain list of records built from w.objects at file level and reset in init
codes: []
fixture: adventure
---
Enemies, pickups and villagers drawn in Tiled are read once, at file level: loop over the world's
`objects`, pick by Class (`o.type`), and add one record per object, counted in a [[makeBounded]] count and
written by index ([[snes.unproven_index]]), keeping its home spot
(`hx`, `hy`) beside its moving position. Read custom properties from `o.props` with a default
(`o.props.look or 1`). The list never grows at run time. Because module values outlive a scene switch,
`init` puts every record back at its spot, alive; a level that restarts runs the same reset. Every lasting
field gets its honest bound; positions on the map are bounded by the map size. Actors that spawn and die
during play come from [[actors-that-come-and-go-are-a-fixed-list|a fixed list made once]] instead.

```luau
local world = require("@ppu/world")
local w = world.load("overworld")

local list = {}
local count = makeBounded(0, 0, 32)
for _, o in w.objects do
  if o.type == "wanderer" or o.type == "charger" then
    local x, y = o.x + o.w // 2, o.y + o.h
    count += 1
    list[count] = {
      hx = x, hy = y,
      x = makeFixed(x, 0, 2047, 8), y = makeFixed(y, 0, 2015, 8), -- MAP SIZE
      hp = makeBounded(1, 0, 2), charger = o.type == "charger",
    }
  end
end

function init()
  for _, e in list do
    e.x, e.y = e.hx, e.hy
    e.hp = if e.charger then 2 else 1
  end
end

function frame(t, f)
  for _, e in list do
    e.x = math.clamp(e.x + 0.125, 0, 2047)
  end
end
```
