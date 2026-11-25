---
id: snes.pool
kind: diagnostic
title: Tables built while the game runs share a fixed pool, so make records once and reuse them
codes: [snes.pool]
---
A cartridge gives the tables a constructor (`{x = 0, y = 0}`) builds a fixed number of slots, chosen
when it is built. The warning `keeps N tables live at once ... but its pool has the default M slots`
means the code can hold more of those tables at the same time than the pool has room for, so the
cartridge would run out where the preview keeps going.

Build the records once, at file level, and change their fields each frame instead of building new
ones: the count is then fixed and nothing is allocated while the game runs. A list grown by
appending (`E[#E + 1] = ...`) stops at its own length limit too; fill it by a bounded index
(`E[i] = ...`) instead.

```luau
local sparks = {}
for i = 1, 8 do
  sparks[i] = {x = makeBounded(0, 0, 255), y = makeBounded(0, 0, 223)}
end

function frame(t, f)
  for i = 1, 8 do
    local s = sparks[i]
    s.x = (i * 32 + f) % 256
    s.y = (i * 24) % 224
  end
end
```
