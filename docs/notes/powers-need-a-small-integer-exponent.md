---
id: powers-need-a-small-integer-exponent
kind: rule
title: "`a ^ b` compiles for a cartridge only when b is a whole number with at most 8 values, all between 0 and 16"
codes: [snes.unsupported]
---
A cartridge has no general power. `2 ^ n` works when `n` is a whole number whose range spans at most 8 values
from 0 up to 16 (`2 ^ (f % 8)`), and the result is a fraction-capable number. A fractional exponent or a wider
range is refused; read the results from a table instead, `P2[n + 1]`.

```luau
local P2 = {1, 2, 4, 8, 16, 32, 64, 128, 256, 512}
local step = makeBounded(0, 0, 9)

function frame(t, f)
  local pulse = 2 ^ (f % 8) -- 8 exponents, 0..7: fine
  step = f % 10
  local far = P2[step + 1] -- 10 values: a table
  brightness = math.min(15, math.floor(pulse / 16) + far % 2)
end
```
