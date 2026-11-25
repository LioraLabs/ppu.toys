---
id: wrap-with-modulo-or-mask-to-prove-a-range
kind: rule
title: "`x % N` and `x & MASK` with literal N and MASK are in range whatever x is, and so is a min or max against a literal"
codes: [snes.unproven_index]
---
A cartridge needs to know every table key and every stored value stays in its range. Some forms prove it on
their own, from nothing known about `x`: `x % 8` is 0 to 7, `x & 255` is 0 to 255, `math.min(x, 10)` is at
most 10 and `math.max(1, x)` at least 1. The frame counter `f` and the time `t` are known only to be
non-negative, so `T[f]` keeps its check while `T[f % 64 + 1]` drops it. `x // 4` is bounded only when `x` is,
and a division or remainder by a variable only when that variable is bounded away from zero. Wrap or clamp
at the point of use, see [[clamp-table-index]].

```luau
local GLOW = {0, 2, 4, 6, 8, 6, 4, 2}

function frame(t, f)
  brightness = 7 + GLOW[f % 8 + 1] -- f % 8 is 0..7 whatever f is: no check on the key
end
```
