---
id: snes.routine
kind: diagnostic
title: yield pauses only a routine's body, for a whole number of updates
codes: [snes.routine]
---
`error[snes.routine]: yield is only allowed inside a routine` means `yield` ran outside the body of a
[[routine]]: in `frame`, in a plain function, in an [[hdma]] callback or in a [[lookup]] function.
`yield needs a positive whole number of updates` means its count was zero, negative or a fraction.

Work spread over several updates goes in a routine: `start` it once with its arguments, `step` it
once an update, and `yield()` in its body where it should pause. Its locals and loop position survive
each pause. A routine has the methods `start`, `step`, `cancel` and `running`. A loop may yield when it is a numeric `for`, `for i, v in ipairs(list)` naming both, `while` or `repeat`; a `pairs` loop that yields is refused.

```luau
local fade = routine(function(to: number)
  for b = 15, to, -1 do
    brightness = b
    yield(2)
  end
end)

function frame(t, f)
  if pad.start then fade:start(0) end
  if fade:running() then fade:step() end
end
```
