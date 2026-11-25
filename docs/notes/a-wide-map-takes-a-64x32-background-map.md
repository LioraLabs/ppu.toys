---
id: a-wide-map-takes-a-64x32-background-map
kind: gotcha
title: A map wider than 32 cells streams into a 64x32 background map, so each bound layer needs 0x800 words of its own that nothing else uses
codes: []
fixture: platformer
---
A scrolling screen shows 33 columns, which do not fit a 32-wide map, so a layer of a map wider than 32 cells streams into a 64x32 background map (`screen_size` 1, 0x800 words from `map`); a narrower map takes one
32x32 screen (0x400 words). Give every bound background its own `map` address, a multiple of 0x400 clear of other placements: `w:bind` refuses a map that overlaps the one another background of the same world uses. A generated world takes
the 64x32 map whatever its templates' width. Bind sets the background's `map_base`, `char_base` and `screen_size`, so do not set them yourself, and follow once in `init()` so the first picture draws whole ([[tiled-maps-for-big-levels]]).

```luau
local world = require("@ppu/world")
local _tiles = dma("tiles", {palette = "tiles_pal", char = 0x1000, pal = 32})
local w = world.load("meadow")

function init()
  mode = 1
  -- meadow is wider than 32 cells: bg[2] takes 0x0000..0x07ff
  w:bind("ground", bg[2], {map = 0x0000})
  w:follow(0, 0)
  screen.main.bg2 = true
end

function frame(t, f)
  w:follow(0, 0)
end
```
