---
id: walk-a-path-of-named-points
kind: recipe
title: A map walker follows Tiled objects named in walking order (start, elder, baker) stop to stop
codes: []
fixture: adventure
---
A path is a list of named objects in Tiled: the stops and the turns between them, each with its own name.
Read them once at file level with `w:find(name)` into one ordered list. Walking is a step straight at the
next point, both axes toward it (`sign(q.x - x)`), at a speed that divides every distance so the walker
lands on each point exactly; on arrival he turns to the following point, or stands at the last. Put every
point on an even pixel (a tile corner keeps the runs straight). See [[@ppu/world]] for loading the map.

```luau
local world = require("@ppu/world")
local w = world.load("overworld")
local P = {w:find("start"), w:find("elder"), w:find("baker")}
local SPEED = 2
local x = makeBounded(P[1].x, 0, 2047) -- MAP SIZE
local y = makeBounded(P[1].y, 0, 2015)
local to = makeBounded(0, 0, 3)

local function sign(v) return if v > 0 then 1 elseif v < 0 then -1 else 0 end

function frame(t, f)
  if to == 0 and pad.pressed.right and x == P[1].x and y == P[1].y then to = 2 end
  if to ~= 0 then
    local q = P[to]
    x, y = x + sign(q.x - x) * SPEED, y + sign(q.y - y) * SPEED
    if x == q.x and y == q.y then
      to = if to < #P then to + 1 else 0
    end
  end
end
```
