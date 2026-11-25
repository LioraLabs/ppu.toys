---
id: clamp-table-index
kind: gotcha
title: Clamp a computed table index into the table's rows so the cartridge can size the table and drop the check
codes: []
---
A table indexed by a computed key (`cy * CORNERS + cx + 1`) needs the cartridge to know every key lands
inside the table. When it cannot prove that, it warns ([[snes.unproven_index]]) and keeps a check on each
use. Wrap the key in `math.clamp(key, 1, n)` with the table's size: the range is then plain, the table is
sized from it, and a wrong key reads the last row instead of past the end.

```luau
local CORNERS = 49
local ON = {}
for c = 1, 2107 do ON[c] = false end

local function on_path(cx: number, cy: number): boolean
  return ON[math.clamp(cy * CORNERS + cx + 1, 1, 2107)]
end

local lit = makeBounded(0, 0, 1)

function frame(t, f)
  lit = if on_path(f % 49, 3) then 1 else 0
end
```
