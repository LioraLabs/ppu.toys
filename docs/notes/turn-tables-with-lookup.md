---
id: turn-tables-with-lookup
kind: recipe
title: Sines and cosines for a turning object come from a lookup over 256 headings
codes: []
---
Store a heading as a bounded integer, 256ths of a turn (0 up, 64 right), and read its sine and cosine
from a [[lookup]]: a cartridge precomputes the table at build time, so a frame's turn is one read, not
two trig calls. A lookup may return several values. Wrap the heading yourself; y grows down the screen,
so moving along it is `(sin, -cos)`.

```luau
local TURN = lookup({heading = {0, 255}}, function(heading)
  return math.sin(heading * math.pi / 128), math.cos(heading * math.pi / 128)
end)

local heading = makeBounded(0, 0, 255)
local x = makeFixed(128, 0, 255, 16)
local y = makeFixed(112, 0, 223, 16)

function frame(t, f)
  if pad.right then heading = (heading + 1) % 256 end
  if pad.left then heading = (heading + 255) % 256 end
  local s, c = TURN(heading)
  x = math.clamp(x + s, 0, 255)
  y = math.clamp(y - c, 0, 223)
end
```
