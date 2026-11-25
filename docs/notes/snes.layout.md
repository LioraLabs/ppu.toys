---
id: snes.layout
kind: diagnostic
title: A table written as a list of values holds only numbers or booleans; a record of fields is written with names
codes: [snes.layout]
---
`error[snes.layout]` means a table constructor is neither a plain list of values nor a record the cartridge can lay
out, most often one that mixes positional values with named fields (`{16, 32, speed = 2}`) or uses `[key] =` entries. A cartridge stores a list of numbers as one array and a
record as named fields, each laid out ahead; a table that is both has no layout. Write a list as positional values
only, or a record as named fields only, and keep several records in a list of records.

```luau
-- Was: local spawn = {16, 32, speed = 2}
local points = {16, 32, 48}
local spawn = {x = makeBounded(16, 0, 255), speed = makeBounded(2, 0, 4)}

function frame(t, f)
  spawn.x = points[f % 3 + 1]
  bg[1].scroll.x = spawn.x
end
```
