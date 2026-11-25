---
id: snes.map
kind: diagnostic
title: A cartridge writes tilemap cells and never reads them back, so a game keeps its own copy of what it drew
codes: [snes.map]
---
A cartridge keeps `bg[n].map` in video memory, where a cell is written once, whole, and cannot be
read back. So it refuses a read of a cell, a test of a column (`bg[n].map[x] == nil` cannot tell
whether you drew there), and holding `bg[n].map` in a local. Keep your own table of what you drew
and read that. Write a cell as `bg[n].map[x][y] = {tile = ...}`, where `x` is the column and `y`
the row; `bg[n].map[x] = {}` clears a column. A level wider than the screen is a
[[tiled-maps-for-big-levels|Tiled map]] drawn by [[@ppu/world]], not cells written by hand.

The layer's `map_base` and `screen_size` must each be one constant, assigned directly in `init()` (not
under an `if`, in a loop, in a helper such as a shared `setup()`, or after a `return`), so the build knows
where the cells go. Write a layer's cells through `bg[n].map` or through `vram[]`,
never both: the preview and a cartridge order the two differently. `m7.map` is refused: make the
Mode 7 map an image source and [[place-art-with-dma|place it with dma()]]. Other `vram[]` writes,
@ppu/hud text among them, [[vram-writes-miss-the-map-cells|must miss the cells]], and in a game of
scenes [[each-scene-sets-the-map-base-it-writes|each scene that writes cells sets the layer's base]].

A cell is written whole, as a table literal in a statement of its own: `bg[n].map[x][y] = {tile = t, pal = p}`,
with the layer number written in place (1 to 4), integer `x` and `y`, and `floor()` on a fraction. Its
fields are `tile`, `pal`, `prio`, `flip_x` and `flip_y`; the two flips are booleans, each set once. To empty a
cell write `{tile = 0}` over it. `bg[n].map = {}` erases every cell drawn so far in the preview, which a cartridge
cannot do: delete the line when nothing is drawn yet (a map starts empty), or clear in `init()`, which runs once
at power-on.

```luau
local drawn = {}
for x = 0, 31 do drawn[x + 1] = 0 end

function init()
  bg[1].map_base = 0x7c00
  bg[1].screen_size = 0
  for x = 0, 31 do bg[1].map[x] = {} end
end

function frame(t, f)
  local x = f % 32
  if drawn[x + 1] == 0 then
    bg[1].map[x][20] = {tile = 1}
    drawn[x + 1] = 1
  end
end
```
