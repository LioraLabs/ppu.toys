---
id: snes.precision
kind: diagnostic
title: A fixed-point value drawn in whole pixels may keep a finer step than the display shows
codes: [snes.precision]
---
The warning `` `x` is rounded to whole drawing units`` means a [[makeFixed]] value is only ever
drawn as whole pixels, yet stored in fine steps (1/1024 by default). A coarser step, such as 1/16,
takes less work RAM. It is your choice, not a fix: the offered edit changes the stored values, so
motion, collisions and timers can play differently. Pick the step in your game's own units, test
the result, and measure speed separately; narrower storage does not promise faster updates.

```luau
local x = makeFixed(32, 0, 240, 16)
local speed = makeFixed(1.5, -4, 4, 16)

function frame(t, f)
  local moved = x + speed
  if moved < 0 or moved > 240 then speed = -speed else x = moved end
  bg[1].scroll.x = math.floor(x)
end
```
