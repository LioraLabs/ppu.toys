---
id: sprite-groups-stack-in-creation-order
kind: rule
title: Sprite groups stack in the order they are made, the first on top
codes: []
fixture: twinstar
---
Which sprite covers which is decided by the order the [[sprites.group|groups]] were made, not the order
they are drawn in a frame: the first group made draws over every later one. Make the groups of one scene
in one ordered list (the top first), so the stacking reads in one place. When groups are made in several
files, the order is the order those files load, which is easy to break by moving a `require`; keep the
list in [[sprite-groups-live-in-groups-luau|groups.luau]] when more than one file draws them. Inside one
group, a [[sorted-group-draws-lower-in-front|sorted group]] orders its own sprites.

```luau
local sheet = dma("ships", {palette = "ships_pal", char = 0x4000, pal = 0})
local SHIP = sprites.art({{size = 16, columns = 1, cells = {0}}}, sheet)
local DRONE = sprites.art({{size = 16, columns = 1, cells = {6}}}, sheet)

-- Top first: ships fly over the enemies, whatever order frame draws them in.
local ships = sprites.group()
local enemies = sprites.group()

function init()
  mode = 1
  screen.main.obj = true
  obj.char_base = sheet.char
end

function frame(t, f)
  enemies:draw(DRONE, 120, 100)
  ships:draw(SHIP, 128, 104)
end
```
