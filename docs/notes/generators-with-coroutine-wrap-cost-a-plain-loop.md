---
id: generators-with-coroutine-wrap-cost-a-plain-loop
kind: recipe
title: A for-in loop over a coroutine.wrap generator costs on a cartridge what the loop inside the generator costs
codes: []
---
A file-level function whose whole body is `return coroutine.wrap(function() ... end)` is a generator: a
`for a, b in gen(args) do` loop over it is written into the generator's own loop on a cartridge, so it makes
no closure and copies nothing. Use the generator only as that loop's header (never store or pass it), put each
`coroutine.yield(...)` as a statement of the generator's body, and keep a `break` in the loop only when all
the yields sit in one loop. The generator is defined once, has no `return` of its own (it would end the
function around the loop), and yields values that cannot be nil: a nil first value ends the loop in the
preview but would run its body once more on a cartridge.

```luau
local xs = {makeBounded(0, 0, 255), makeBounded(0, 0, 255), makeBounded(0, 0, 255), makeBounded(0, 0, 255), makeBounded(0, 0, 255), makeBounded(0, 0, 255), makeBounded(0, 0, 255), makeBounded(0, 0, 255)}

local function lit(list)
  return coroutine.wrap(function()
    for i, x in ipairs(list) do
      if x > 0 then coroutine.yield(i, x) end
    end
  end)
end

function frame(t, f)
  xs[f % 8 + 1] = f % 2 * 100
  for i, x in lit(xs) do
    xs[i] = x - 1
  end
end
```
