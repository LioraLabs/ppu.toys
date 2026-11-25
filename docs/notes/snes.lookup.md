---
id: snes.lookup
kind: diagnostic
title: A lookup is a table over a grid of named inputs, and a call outside the grid is an error
codes: [snes.lookup]
---
[[lookup|`lookup(grid, function(...) end)`]] computes its function once for every point of the grid
and keeps the results in ROM; on a cartridge a call is a table read. `error[snes.lookup]` names what
the grid cannot hold:

- `grid has no range for parameter x`, or `names an axis that is not a callback parameter`: the grid's
  keys and the function's parameters match by name, see [[grid-keys-by-name]].
- `needs min <= max and a positive step that lands on max`: `{0, 255}`, or `{0, 252, 4}` with a step.
- `input x=N is outside lo..hi`: the preview stops as the cartridge would. Clamp or wrap the value
  before the call; inside the grid a value between points is rounded to the nearest one.
- `at most five inputs`, `needs its function written inline`.

The note `Baked NAME: N entries, B ROM bytes` is a report, not a problem: it says what the table
costs. Results keep 1/1024 precision; round them yourself (`math.floor(v + 0.5)`) when an integer will
do, and see [[snes.precision_cost]] for what precise results cost to multiply. A lookup returns the same number of values at every point of its grid, or it is refused when built; outputs the code never reads take no ROM, and fractional outputs that every use floors (`math.floor`, a sprite's x or y) are stored as whole numbers.

```luau
local TURN = lookup({heading = {0, 255}}, function(heading)
  return math.sin(heading * math.pi / 128), math.cos(heading * math.pi / 128)
end)

local heading = makeBounded(0, 0, 255)

function frame(t, f)
  heading = (heading + 1) % 256
  local s, c = TURN(heading)
  m7.a, m7.b, m7.c, m7.d = c, -s, s, c
end
```
