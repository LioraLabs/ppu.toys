---
id: platformer-jump-feel
kind: recipe
title: A platformer feels right with momentum, a jump cut short on release, coyote time and a jump buffer
codes: []
---
Four small rules make a run and jump feel fair. Momentum: holding a side adds a little speed a frame
up to a top speed, and letting go takes a little away. A variable jump: rising with the button let go
adds extra gravity, so a tap is a hop and a hold is a full jump. Coyote time: a jump still works for a
few frames after the feet leave a ledge. A jump buffer: a press a few frames before landing still
jumps on landing.

Every value is in pixels and frames, kept with [[makeFixed]] and [[makeBounded]] at its honest range,
and clamped where it can grow. The landing itself comes from the tiles
([[move-one-axis-then-snap|moving against tiles]]); here the floor is a line at y 176.

```luau
local RUN, ACCEL, FRICTION = 2.25, 0.375, 0.25
local JUMP, GRAVITY, MAX_FALL, CUT, GRACE = 5.25, 0.25, 5, 0.375, 6
local x, y = makeFixed(40, 0, 255, 8), makeFixed(176, 0, 176, 8)
local vx, vy = makeFixed(0, -3, 3, 8), makeFixed(0, -6, 6, 8)
local coyote, buffer = makeBounded(0, 0, 6), makeBounded(0, 0, 6)

function frame(t, f)
  local dir = (if pad.right then 1 else 0) - (if pad.left then 1 else 0)
  if dir ~= 0 then
    vx = math.clamp(vx + dir * ACCEL, -RUN, RUN)
  elseif vx > 0 then
    vx = math.max(0, vx - FRICTION)
  else
    vx = math.min(0, vx + FRICTION)
  end
  buffer = if pad.pressed.b then GRACE else math.max(0, buffer - 1)
  coyote = math.max(0, coyote - 1)
  if buffer > 0 and coyote > 0 then vy, buffer, coyote = -JUMP, 0, 0 end
  if vy < 0 and not pad.b then vy = math.min(0, vy + CUT) end
  vy = math.min(MAX_FALL, vy + GRAVITY)
  x = math.clamp(x + vx, 0, 255)
  y = math.clamp(y + vy, 0, 176)
  if y == 176 then vy, coyote = 0, GRACE end
end
```
