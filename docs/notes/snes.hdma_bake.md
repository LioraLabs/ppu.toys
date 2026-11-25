---
id: snes.hdma_bake
kind: diagnostic
title: bakeHdma bakes a scanline effect into ROM only when its rows depend on the row and its grid inputs alone
codes: [snes.hdma_bake]
---
[[bakeHdma]] keeps one table of rows per grid point in ROM, so a call each frame only copies one. The
code reports what was baked (`Baked NAME over N grid points`, with the video frames each call's copy takes) and, as a
warning or a refusal, what was not:

- `its callback reads ...` or `Not baked, computed per update instead`: a row reads something besides
  `y`, its grid inputs, constants and ROM tables (a frame counter, a moving position). Make that input
  a grid axis, or set it once in `frame()`.
- `bakeHdma bakes only the mode, the Mode 7 matrix, one layer's scroll, color.fixed, and either per-row
  Mode 7 centers or window edges`: the callback writes another register (`screen.main`, a second
  layer's scroll, a palette entry). Set it once in `frame()` or `init()`, or give that effect its own
  [[hdma]] schedule (see [[snes.hdma_live]] for what a live schedule costs).
- `writes the window edges ... but not the others`: write `w1.lo` with `w1.hi` (and `w2` likewise), or
  all four, or write the other edges too (the frame's value, if it holds) so the edges one program writes are one run.
- `writes ... but not ..., and that table is not baked`: one program writes only half of a table's streams,
  and the cartridge fills the table per update, both halves of every row. Write the other half in that program
  too, or make every input of the table's rows a grid axis so it bakes.
- `No table of these bakeHdma programs bakes`: several programs share a channel and none of their tables bakes,
  so the cartridge cannot tell which program a frame ran. Make every input of the rows a grid axis.
- `cannot be evaluated`: a row's maths fails for some grid value; bound the inputs to the values the
  game reaches, or clamp the maths.

Grid errors (`grid has no range for parameter`, `input outside`) are the same as [[snes.lookup]]'s,
and the grid's keys match the callback's parameters by name ([[grid-keys-by-name]]).

```luau
local wave = bakeHdma({phase = {0, 31}}, 0, 223, function(y, phase)
  bg[1].scroll.x = math.floor(math.sin((y + phase * 2) * math.pi / 16) * 3)
end)

function frame(t, f)
  screen.main.bg1 = true -- holds for the whole frame: set here, not in the callback
  wave(f % 32)
end
```
