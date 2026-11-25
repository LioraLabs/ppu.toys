---
id: snes.bounded
kind: diagnostic
title: makeBounded and makeFixed take literal bounds and initialize a named binding directly, once
codes: [snes.bounded]
---
`error[snes.bounded]` means a [[makeBounded]] or [[makeFixed]] call is not in the form both the preview and the cartridge
read the same way. Each one:

- initializes a named binding or a record field directly (`local x = makeBounded(...)`), never nested in another
  expression or passed as an argument;
- has an initial value and two whole-number literal bounds, minimum first; `makeFixed` may add a step denominator, a
  power of two from 1 to 1024, and its bounds must fit −2,097,152..2,097,151;
- declares a global only at file level, outside any `if` or loop; elsewhere use a `local`.

Later stores are plain assignments: write `x = x + 1`, not `x = makeBounded(x + 1, 0, 9)`.

```luau
local lives = makeBounded(3, 0, 9)
local speed = makeFixed(0, -4, 4, 16)

function frame(t, f)
  if pad.a then speed = math.clamp(speed + 0.5, -4, 4) end
  if pad.b and lives > 0 then lives = lives - 1 end
end
```
