---
id: sway-from-a-periodic-lookup
kind: recipe
title: A sway or bob animation is a periodic lookup indexed by f % period
codes: []
---
For motion that repeats (a cursor bobbing, ships leaning, a camera swaying) make a [[lookup]] over one
period and index it with the frame count modulo that period. The table is built ahead of time, the index
needs no state of its own, and one table can return several rounded offsets at once.

```luau
local SWAY = lookup({t = {0, 63}}, function(t)
  local a = t * math.pi / 32
  return math.floor(math.sin(a) * 3 + 0.5), math.floor(math.cos(a) * 2 + 0.5)
end)

function frame(t, f)
  local sway, float = SWAY(f % 64)
  bg[1].scroll.x = sway
  bg[1].scroll.y = float - 4
end
```
