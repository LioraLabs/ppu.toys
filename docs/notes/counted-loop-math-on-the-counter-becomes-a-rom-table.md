---
id: counted-loop-math-on-the-counter-becomes-a-rom-table
kind: concept
title: Costly math in a counted loop of at most 256 steps that reads only the counter and constants is a table read on a cartridge
codes: []
---
In a `for` loop with literal limits, at most 256 steps of 1 and a counter it never assigns, an expression that
uses only the counter and constants (a division, a remainder, a multiply by a non-power of two, `math.sin`,
`math.cos`, `math.sqrt`, `rgb`) is worked out when the cartridge is built and read from ROM, one load a step.
Read game state into it and it is computed at run time again. So write per-row and per-step shapes (a
perspective divide, a circle's edge, a colour ramp) over the counter alone, and add the moving part after.

```luau
function frame(t, f)
  for i = 0, 15 do
    -- rgb() of the counter alone: worked out at build time, one ROM read a step
    cgram[16 + i] = rgb(i * 16, 128, 255 - i * 16)
  end
end
```
