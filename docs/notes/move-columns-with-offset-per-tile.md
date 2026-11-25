---
id: move-columns-with-offset-per-tile
kind: recipe
title: In Mode 2, bg[n].columns moves each 8-pixel column of a layer up or down on its own
codes: [snes.columns]
---
In Mode 2 each 8-pixel column of BG1 or BG2 can scroll up or down on its own: pipes with gaps at
different heights, a wavy floor, a curtain. `bg[n].columns[c] = dy` adds `dy` pixels to the layer's
`scroll.y` for map column `c` (0 to 31, or 0 to 63 on a 64-wide map), and it stays until you change it.
Write `0` to put a column back: a cartridge refuses `nil`. The column belongs to the map, not the
screen, so scroll the layer sideways and the moved columns go with it. A scene change clears them.

Every frame the engine builds the offset table from the columns and the layer's scroll and writes it
into BG3's map at `bg[3].map_base`, 64 words: give BG3 a spot nothing else uses, and keep `vram[]`
writes off it. Use `bg[n].columns` or write the table yourself with `vram[]`, never both. Columns move
one layer at a time, with `tile_size` 8 and `bg[3].scroll` left at 0, and only in Mode 2. The table
follows the scroll `frame()` sets; a scroll changed mid-screen by `hdma()` does not move the columns.

The hardware never moves the leftmost screen column, so a moved column shows unmoved as it scrolls
out on the left. Hide that strip with a [[window-hides-a-layer-inside-a-span|window]] over x 0 to 7.
Columns are write-only on a cartridge: keep your own array when you need to read an offset back.

```luau
function init()
  mode = 2                  -- offset-per-tile lives in Mode 2
  bg[3].map_base = 0x7800   -- BG3's map holds the offset table
  bg[2].map:fill(4, 20, 2, 8, {tile = 1})
  screen.main.bg2 = true
  win.w1.lo, win.w1.hi = 0, 7 -- the leftmost column never moves: hide it
  win.bg2.w1, win.bg2.main = true, true
  bg[2].columns[4] = 40     -- map columns 4 and 5 show 40 pixels further down the map
  bg[2].columns[5] = 40
end

function frame(t, f)
  bg[2].scroll.x = f % 256  -- the moved columns travel with the map
end
```
