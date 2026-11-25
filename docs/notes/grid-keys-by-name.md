---
id: grid-keys-by-name
kind: gotcha
title: bakeHdma and lookup match each callback parameter to a grid key by its exact name, so _phase is not phase
codes: []
---
A [[bakeHdma]] or [[lookup]] grid is a table of ranges keyed by name, and each parameter of the
callback (after `y`, for `bakeHdma`) takes its range from the key spelled the same way. Nothing is
matched by position: renaming `phase` to `_phase` to mark it unused leaves it without a range, and
the call fails with `grid has no range for parameter _phase`. A grid key with no parameter of that
name fails too: `grid names an axis that is not a callback parameter`.

Spell each parameter as its key. The error is about names only; it says nothing about whether the
effect can be baked. An input that never changes needs no grid at all: a schedule that reads only
its row and constants is filled once, so write it as a plain [[hdma]].

```luau
local wave = bakeHdma({phase = {0, 31}}, 0, 223, function(y, phase)
  bg[1].scroll.x = (y + phase) % 8
end)

function frame(t, f)
  wave(f % 32)
end
```
