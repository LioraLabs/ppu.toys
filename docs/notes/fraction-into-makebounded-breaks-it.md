---
id: fraction-into-makebounded-breaks-it
kind: gotcha
title: A fraction stored into a makeBounded value breaks its contract, because makeBounded holds whole numbers only
codes: []
---
[[makeBounded]] states a whole number. Storing a fraction into it (`x = x + 0.5`, a speed times 0.75, a
division that does not come out even) is a broken store, the same as going outside the range: the preview
lists it as a [[contract]] violation and a cartridge stops. Nothing rounds or clamps for you.

Declare any value that carries a fraction with [[makeFixed]], which keeps steps of 1/1024 or the coarser
step you choose. Keep a counter, a score or a tile index in `makeBounded`, and floor a fraction (`//`)
before it goes in.

```luau
local x = makeFixed(32, 0, 240, 16)
local speed = makeFixed(0.5, -4, 4, 16)
local ticks = makeBounded(0, 0, 255)

function frame(t, f)
  x = math.clamp(x + speed, 0, 240)
  ticks = (ticks + x // 8) % 256
  bg[1].scroll.x = x // 1
end
```
