---
id: fixed-step-and-speeds
kind: rule
title: Pick one makeFixed step for a value and keep every speed added to it a multiple of that step
codes: []
---
[[makeFixed|makeFixed(value, min, max, steps)]] keeps a value in steps of 1/steps (a power of two up to
1024). Choose the step from the smallest change the game needs: 1/8 of a pixel for a walking hero, 1/16
for a ship or a bolt, 1/64 for a car that speeds up slowly. Then write every speed, acceleration and drag
as a multiple of it (`1 / 32`, `1 / 64` for a 1/64 step). A change finer than the step is rounded on
every store: a slow push rounds to nothing and the value never moves, or creeps unevenly.

A smaller step is not free: a value's range times its steps sets how much memory it takes. The bounds
stay [[bounds-are-literals|literal]].

```luau
local TOP_SPEED = 3
local ACCELERATE = 1 / 32 -- multiples of 1/64, the step speed is kept in
local COAST = 1 / 64

local car = {
  x = makeFixed(512, 512, 3584, 16),
  speed = makeFixed(0, 0, 3, 64),
}

function frame(t, f)
  local speed = if pad.b then car.speed + ACCELERATE else car.speed - COAST
  car.speed = math.clamp(speed, 0, TOP_SPEED)
  car.x = math.clamp(car.x + car.speed, 512, 3584)
end
```
