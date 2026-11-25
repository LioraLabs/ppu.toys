---
id: math-random-draws-integers
kind: rule
title: math.random on a cartridge draws whole numbers; math.random() with no arguments is refused
codes: []
---
`math.random(n)` draws 1 to n and `math.random(a, b)` draws a to b, on a cartridge and in the Studio from the
same generator. `math.random()` with no arguments returns a fraction, which a cartridge does not draw: scale a
whole number instead, `math.random(0, 1023) / 1024`, into [[makeFixed]] state. For randomness tied to a place
rather than a moment, use `math.hash(seed, x, y)` ([[scatter-by-place-with-math-hash]]).

```luau
local drift = makeFixed(0, 0, 1, 1024)
local roll = makeBounded(1, 1, 6)

function frame(t, f)
  if pad.pressed.a then
    roll = math.random(6)
    drift = math.random(0, 1023) / 1024
  end
end
```
