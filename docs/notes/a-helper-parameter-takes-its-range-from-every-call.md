---
id: a-helper-parameter-takes-its-range-from-every-call
kind: rule
title: A helper's number parameter is in range only when every call passes a bounded value and the helper never reassigns it
codes: [snes.unproven_index]
---
A cartridge works out a helper's parameter range from all its calls together. When every call passes a
value it can bound, the parameter has that range, and a table read or store with it needs no check. One call
that passes something unbounded (raw `f`, an unclamped sum) leaves the parameter unbounded for every call; so
does assigning the parameter inside the helper. Clamp at the loose call, and copy a parameter you need to
change into a local.

```luau
local ROW = {}
for i = 1, 32 do ROW[i] = 0 end

local function mark(col)
  local c = col -- change the copy, never col itself
  ROW[c] = 1
end

function frame(t, f)
  mark(f % 32 + 1)      -- bounded 1..32
  mark(math.min(32, 4)) -- bounded too: every call agrees, so ROW[c] needs no check
end
```
