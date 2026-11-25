---
id: snes.hdma_live
kind: diagnostic
title: An hdma() schedule that reads t, f, the pad or a run-time table recomputes every row on every update
codes: [snes.hdma_live]
---
The warning `This hdma(first, last, ...) schedule computes all N rows on every update` means the
callback reads something a cartridge must assume changed on every update: `t`, `f`, the pad, a
table the program writes at run time, a function call, or rows another `hdma()` schedule also writes.

What decides the cost is what the callback reads, never which registers it writes; scroll, mode,
Mode 7 and colour schedules all follow the same rule:

- Only its row, constants and ROM tables: the rows are filled once, and later updates cost nothing
  per row, whether the callback writes one register or four.
- A scalar (a camera position, a radius): every row is filled again, but only on updates where that
  scalar changed. No warning; a still scalar costs nothing.
- `t`, `f` or the rest above: every row of every register it writes is filled again on every
  update, each extra register adding to each row's cost.

Fix it by moving the changing input out of the callback: when the effect has one input with a known
range, [[bake-scanline-effects|bake it]] with [[bakeHdma]] (what bakes: [[snes.hdma_bake]]) and call the baked function each frame. Keep the
live [[hdma]] only when the effect truly must move every row every update, and accept its cost.

```luau
-- Was: hdma(0, 223, function(y) bg[1].scroll.x = (y + f) % 8 end), reading f every update.
local shift = bakeHdma({phase = {0, 7}}, 0, 223, function(y, phase)
  bg[1].scroll.x = (y + phase) % 8
end)

function frame(t, f)
  shift(f % 8)
end
```
