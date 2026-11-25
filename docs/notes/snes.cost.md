---
id: snes.cost
kind: diagnostic
title: Division, remainder, trigonometry and square roots reached from frame() are computed by the 65816 on every update
codes: [snes.cost]
---
`NAME contains N runtime division/remainder, trig or square-root expressions reachable from frame`
counts the expressions the cartridge computes in software each time they run: the 65816 has no
divider fast enough for game maths and no sine at all, and a loop or call repeats each one. The count
is not a cycle estimate; [[snes.budget]] says whether the update still fits.

When the expression is a pure function of inputs with a known range, compute it ahead of time with
[[lookup]]: the cartridge keeps every result in ROM and the call becomes a table read. Choose the
grid's steps yourself (64 angles, not 360) and compare the picture. The same code also flags clearing a whole map
column from `frame()`: clear it in `init()`, or overwrite only the cells that changed.

```luau
-- Was: local r = math.sqrt(dx * dx + dy * dy) each update, dx and dy in 0..15.
local DISTANCE = lookup({dx = {0, 15}, dy = {0, 15}}, function(dx, dy)
  return math.floor(math.sqrt(dx * dx + dy * dy) + 0.5)
end)

function frame(t, f)
  brightness = 15 - DISTANCE(f % 16, 3) // 2
end
```
