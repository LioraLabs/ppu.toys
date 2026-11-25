---
id: snes.retype
kind: diagnostic
title: A whole number that later takes a fraction is declared fixed point with makeFixed from the start
codes: [snes.retype]
---
A cartridge stores each number either whole or in fixed point, and decides once for the whole
program. `error[snes.retype]: x is retyped` means a value that starts whole (`local x = 0`) later
gets a fraction (`x = x + 0.5`), or a helper's parameter is passed one. Declare such state with
[[makeFixed]] and its honest bounds, so it is fixed point from its first value; pass fractions to a
helper as fixed-point values too.

```luau
local speed = makeFixed(0, 0, 255, 16) -- was: local speed = 0

function frame(t, f)
  speed = math.min(speed + 0.5, 255)
  bg[1].scroll.x = math.floor(speed)
end
```
