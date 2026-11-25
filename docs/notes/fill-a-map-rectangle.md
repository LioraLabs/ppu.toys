---
id: fill-a-map-rectangle
kind: recipe
title: bg[n].map:fill sets a rectangle of map cells to one entry in a single line
codes: [snes.map]
---
`bg[n].map:fill(x, y, w, h, entry)` writes `entry` into every cell of the `w` by `h` rectangle whose
top-left cell is (`x`, `y`): a pipe's body, a floor, a frame around a panel. Each cell gets its own copy
of the entry, which names the same fields as a single cell: `tile`, `pal`, `prio`, `flip_x`, `flip_y`.

The rectangle must lie inside the layer's map (32 by 32 cells, 64 across or down with `screen_size`).
The preview stops at a fill that reaches past it; a cartridge cannot, so it refuses one it cannot prove
inside: write `w` and `h` as numbers in place, the entry as a table literal in place, and bound `x` and
`y` with `makeBounded` or a clamp. Set the layer's `map_base` and `screen_size` above the first fill, as
for [[snes.map|single cells]].

```luau
function init()
  mode = 1
  bg[2].map_base = 0x0000
  bg[2].screen_size = 2                              -- 32 x 64 cells
  bg[2].map:fill(0, 26, 3, 1, {tile = 4, pal = 1})   -- a cap
  bg[2].map:fill(0, 27, 3, 14, {tile = 1, pal = 1})  -- the body under it
  screen.main.bg2 = true
end
```
