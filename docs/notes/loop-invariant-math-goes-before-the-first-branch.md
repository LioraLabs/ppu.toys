---
id: loop-invariant-math-goes-before-the-first-branch
kind: recipe
title: Math that does not change inside a loop is computed once only from the top of its body, before any if, call or inner loop
codes: []
---
A cartridge lifts an expression that is the same on every step of a counted loop (`math.sin(t * 2)`, `row * 20`)
out in front of it and computes it once, but only from the assignments that open the body. After the first
`if`, inner loop or helper call, it stays where it is and runs every step. Compute such values in a local
before the loop, or at the very top of the body.

```luau
local ys = {makeBounded(0, 0, 224), makeBounded(0, 0, 224), makeBounded(0, 0, 224), makeBounded(0, 0, 224), makeBounded(0, 0, 224), makeBounded(0, 0, 224), makeBounded(0, 0, 224), makeBounded(0, 0, 224)}

function frame(t, f)
  local swing = math.floor(math.sin(t * 2) * 8) -- once, before the loop
  for i = 1, 8 do
    if i % 2 == 0 then ys[i] = 100 + swing else ys[i] = 100 - swing end
  end
end
```
