---
id: snes.clamp
kind: diagnostic
title: math.clamp(x, min, max) needs min no greater than max
codes: [snes.clamp]
---
`error[snes.clamp]: math.clamp: max must not be less than min` means a `math.clamp` call got its
bounds the wrong way round, often from swapped arguments or from a range computed at run time that
can turn inside out. The preview stops on it. A cartridge lowers `math.clamp` to a min of a max and
does not check, so the same call there returns `max` whatever `x` is.

Pass the value first, then the low bound, then the high one. When the bounds come from other state,
order them yourself with `math.min` and `math.max` before clamping.

```luau
local x = makeBounded(120, 8, 232)

function frame(t, f)
  if pad.left then x = math.clamp(x - 2, 8, 232) end
  if pad.right then x = math.clamp(x + 2, 8, 232) end
  bg[1].scroll.x = x
end
```
