---
id: snes.columns
kind: diagnostic
title: A cartridge moves bg[n].columns only in Mode 2, on one layer, with a constant table spot clear of every map
codes: [snes.columns]
---
A cartridge keeps `bg[n].columns` in work RAM and rebuilds BG3's offset table from them every update,
so it refuses what it cannot build the same way. Assign `mode = 2`, and only 2, directly in `init()` (or
the init() of the scene that moves columns). Move the columns of `bg[1]` or `bg[2]`, not both, with the
layer written as a number in place. Leave `tile_size` at 8 and `bg[3].scroll` at 0. Give
`bg[3].map_base` one constant value clear of BG1's and BG2's maps (`0x7800` is a common spot), and keep
`c` provably inside the layer's map width with `makeBounded`, a clamp or `% 32`.

Write `0`, never `nil`, to put a column back, and keep your own array if you need to read the offsets:
a cartridge cannot read them back. Keep `vram[]` writes off the 64 words of the table, or write the
table yourself and leave `bg[n].columns` alone. A column cannot be set inside an `hdma()` callback.
See [[move-columns-with-offset-per-tile]].

```luau
local gap = makeBounded(0, 0, 31)

function init()
  mode = 2
  bg[3].map_base = 0x7800
  screen.main.bg2 = true
end

function frame(t, f)
  gap = f // 8 % 32
  bg[2].columns[gap] = 16 -- gap is proven 0..31
end
```
