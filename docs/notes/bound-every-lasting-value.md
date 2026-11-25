---
id: bound-every-lasting-value
kind: rule
title: Every value that lasts between frames is made with makeBounded or makeFixed at its honest range, and the code clamps or wraps it itself
codes: []
---
A cartridge sizes each lasting value's memory from the range [[makeBounded|makeBounded(value, min, max)]]
or [[makeFixed|makeFixed(value, min, max, steps)]] states, and [[snes.halt|stops]] on the first store outside
it, where the preview keeps counting. Write the contract when you write the variable, not after:
positions, speeds, timers, counters, facings, table fields of a record alike.

Make the range honest: every value the code can produce, and no wider than it needs (a wider range costs
memory and time). Then keep the value inside it yourself: `math.clamp` a position to the field,
`math.max(0, t - 1)` a countdown, `% n` a phase that wraps. The bounds must be
[[bounds-are-literals|written as numbers]].

```luau
local LEFT, RIGHT, TOP, BOTTOM = 16, 240, 32, 208
local SPEED = 2

local hero = {x = makeBounded(128, 16, 240), y = makeBounded(120, 32, 208)}
local left = makeBounded(1200, 0, 1200)
local phase = makeBounded(0, 0, 31)

function frame(t, f)
  local dx = (if pad.right then SPEED else 0) - (if pad.left then SPEED else 0)
  local dy = (if pad.down then SPEED else 0) - (if pad.up then SPEED else 0)
  hero.x = math.clamp(hero.x + dx, LEFT, RIGHT)
  hero.y = math.clamp(hero.y + dy, TOP, BOTTOM)
  left = math.max(0, left - 1)
  phase = (phase + 1) % 32
end
```
