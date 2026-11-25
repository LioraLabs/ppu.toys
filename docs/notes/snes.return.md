---
id: snes.return
kind: diagnostic
title: A helper returns the same number of values on every path, and frame() and init() return none
codes: [snes.return]
---
A cartridge has no nil, so a value that is missing on some path cannot be represented. It refuses a
helper that returns a value on one path and ends without one on another (or uses a bare `return`
there), a helper whose returns give different numbers of values, and `frame()` or `init()`
returning a value. It also refuses `{ pair() }` when `pair` returns several values: capture them
first, `local a, b = pair()`, then `{a, b}`.

End every path of a helper with a return that gives the same number of values. To get a value out
of `frame()`, store it in a file-level variable.

```luau
local function sign(n)
  if n > 0 then return 1 end
  if n < 0 then return -1 end
  return 0
end

function frame(t, f)
  bg[1].scroll.x = 8 + sign(f % 3 - 1)
end
```
