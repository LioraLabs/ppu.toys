---
id: scatter-by-place-with-math-hash
kind: recipe
title: math.hash answers for a place, so a cell's contents come out the same every time it is asked
codes: []
---
`math.random` deals a sequence: the answer depends on how many draws came before. `math.hash(seed, x, y)`
keeps no state: the same seed and the same two whole numbers give the same number from 0 to 255 in any
order of calls, in the preview and on a cartridge. Use it for what a place holds: which star sits in a
cell, which flower grows in a column, which room a level number opens on. Another seed (0 to 65535) is
another scatter. A hash argument that is not a whole number stops the preview with an error.

Use [[seed-random-at-start|math.random with a seed]] for what happens next, and `math.hash` for what is
there.

```luau
local seed = makeBounded(7, 0, 9999)

function frame(t, f)
  if pad.pressed.start then seed = f % 10000 end
  -- A palette entry per column: the same colours for the same seed, run after run.
  local column = f // 8 % 32
  cgram[1] = math.hash(seed, column, 0) % 32
end
```
