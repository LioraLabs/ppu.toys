---
id: bake-scanline-effects
kind: recipe
title: A scanline effect with one moving input is baked once per value with bakeHdma, not computed per line each frame
codes: []
---
[[bakeHdma|`bakeHdma(grid, first, last, line)`]] runs `line(y, ...)` for every scanline from `first` to `last`, as
[[hdma|hdma()]] does, but for every point of the grid at once: a cartridge keeps each version in ROM and
picks one per frame. Give each input its honest range (`{0, 31}` for a 32-step phase). The baked
function takes the inputs, not `y`; call it every frame with the current value.

A value outside the grid is rounded to the nearest point, so clamp or wrap the input yourself. Shared
per-line maths (a sky gradient) goes in a [[lookup]] that the bakes call. An [[snes.hdma_live|hdma()
schedule computed every update]] is what this replaces.

The grid's bounds are numbers, or a constant (a file-level `local LO, HI = -8, 8` stored to nowhere
else, or a module's export of one) with `+`, `-` and `*`. The function runs once per row and input
when the cartridge is built, so it depends on those alone, and `y` in it is the row: a helper it calls
must not use the name `y`. A `return` in it comes only as `if condition then ... return end` before the
other rows; write the other cases as `if`/`else`.

```luau
local wave = bakeHdma({phase = {0, 31}}, 0, 223, function(y, phase)
  bg[1].scroll.x = math.floor(math.sin((y + phase * 2) * math.pi / 16) * 3)
end)

function frame(t, f)
  wave(f % 32)
end
```
