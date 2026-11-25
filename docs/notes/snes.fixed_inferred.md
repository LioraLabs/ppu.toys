---
id: snes.fixed_inferred
kind: diagnostic
title: A fraction the code computes without makeFixed becomes 1/1024 fixed point on a cartridge, so it can drift from the preview
codes: [snes.fixed_inferred]
---
`Fractional values are lowered as Q21.10 fixed point` means a file computes with fractions (a
decimal literal, a division whose result is not whole, `t`) that no [[makeFixed]] declares. The
cartridge has no floating point: it keeps such values to the nearest 1/1024, and products and
quotients truncate toward zero, so a value that accumulates over many frames can drift away from what
the preview shows.

Declare every fraction that lasts between frames with `makeFixed(value, min, max, steps)`: it states
the range and the step (1/16 here), the preview then follows the same arithmetic as the cartridge, and
the compiler sizes the value from it. Integer maths (`//`, whole numbers) never needs it.

```luau
local x = makeFixed(32, 0, 240, 16)
local speed = makeFixed(1.5, -4, 4, 16)

function frame(t, f)
  local moved = x + speed
  if moved < 0 or moved > 240 then speed = -speed else x = moved end
  bg[1].scroll.x = x // 1
end
```
