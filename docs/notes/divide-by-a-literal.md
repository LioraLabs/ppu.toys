---
id: divide-by-a-literal
kind: concept
title: Division by a literal is cheap on a cartridge and division by a variable costs about a tenth of a frame
codes: [snes.cost]
---
`x // 8` and `x % 16` are a shift and a mask. `x // 10` or `x % 60` by any literal up to 255 uses the console's
hardware divider, a few hundred cycles. A divisor that is a variable takes a long division, near a tenth of a
60 Hz frame a use; a handful of those in a loop spend a whole frame. Keep divisors literal, use powers of two
where the choice is free, take a quotient once and reuse it, and turn a ratio over a variable into a
[[lookup]]. A variable divisor also needs a zero guard ([[snes.division]]).

```luau
local hp = makeBounded(100, 0, 100)
local bar = makeBounded(0, 0, 25)

function frame(t, f)
  if pad.pressed.a then hp = math.max(0, hp - 7) end
  bar = hp // 4 -- a literal: a shift. hp // maxhp with a variable maxhp would be a long division
end
```
