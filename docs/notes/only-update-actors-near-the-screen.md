---
id: only-update-actors-near-the-screen
kind: recipe
title: Only actors near the screen think and draw; a defeated one returns once its spot is off screen
codes: []
---
A map holds more actors than a frame can afford to run. Test each against the camera with a margin
(`near(x, y, margin)`): only those near the screen [[enemy-think-move-bump|think]], move and draw; the rest
stand still where they are. A defeated actor waits, gone, until its home spot is off the screen, then is
reset there ([[map-placed-actors-are-record-lists|reset from its record]]), so it never pops back in view. A
rooms camera uses a margin of 0 and resets a room's actors when it arrives.

```luau
local cam = {x = makeBounded(0, 0, 1792), y = makeBounded(0, 0, 1792)} -- MAP SIZE
local list = {{hx = 300, hy = 200, gone = false}, {hx = 900, hy = 400, gone = true}}

local function near(x, y, margin)
  return x > cam.x - margin and x < cam.x + 256 + margin and y > cam.y - margin and y < cam.y + 224 + margin
end

function frame(t, f)
  cam.x = math.clamp(cam.x + (if pad.right then 2 else 0) - (if pad.left then 2 else 0), 0, 1792)
  for _, e in list do
    if e.gone then
      if not near(e.hx, e.hy, 16) then e.gone = false end
    elseif near(e.hx, e.hy, 32) then
      -- think, move and draw this one
    end
  end
end
```
