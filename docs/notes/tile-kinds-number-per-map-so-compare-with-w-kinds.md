---
id: tile-kinds-number-per-map-so-compare-with-w-kinds
kind: gotcha
title: A tile class gets its number from its first appearance in each map, so compare with w.kinds.solid and never a number saved from another map
codes: []
fixture: platformer
---
`w.kinds` maps each Class name to a number in the order the classes first appear in the map's tileset, and a tile with no class is 0. After
[[w-enter-cuts-to-another-map-under-force-blank|w:enter]] the world's `w.name`, `w.width`, `w.height`, `w.max_x`, `w.max_y`, `w.kinds`, `w.objects` and `w:tile`
are the new map's, so the same class can be a different number there. Compare against `w.kinds.solid` each time, write `local SOLID = w.kinds.solid` only for a world that never enters another map, and
keep the class names (not the numbers) in any table that outlives an enter. See [[collision-from-tile-classes]].

```luau
local world = require("@ppu/world")
local _tiles = dma("tiles", {palette = "tiles_pal", char = 0x1000, pal = 32})
local w = world.load("meadow")
local px, py = makeBounded(40, 0, 1500), makeBounded(64, 0, 220)

function init()
  mode = 1
  w:bind("ground", bg[2], {map = 0x0000})
  w:follow(0, 0)
  screen.main.bg2 = true
end

function frame(t, f)
  w:follow(0, 0)
  -- w.kinds.solid, read now, is this map's number
  if w:tile("ground", px, py + 16) ~= w.kinds.solid then
    py = math.min(220, py + 1)
  end
end
```
