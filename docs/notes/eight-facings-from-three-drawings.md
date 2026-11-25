---
id: eight-facings-from-three-drawings
kind: recipe
title: Eight facings come from three drawings (north, north-east, east) and two flips
codes: []
fixture: twinstar
---
A top-down ship or character needs only three drawings: facing north, north-east and east. The other five
facings are those flipped: `flip_x` mirrors left and right, `flip_y` up and down. Number the facings 1 to 8
clockwise from north and keep three small tables: which drawing, and whether to flip each way. A draw passes
the flips as options to `group:draw` ([[sprites-group-draw-every-frame|drawing sprites]]); a flip costs nothing.

```luau
local sheet = dma("ships", {palette = "ships_pal", char = 0x4000, pal = 0})
local SHIP = {
  sprites.art({{size = 16, columns = 1, cells = {0}}}, sheet),
  sprites.art({{size = 16, columns = 1, cells = {1}}}, sheet),
  sprites.art({{size = 16, columns = 1, cells = {2}}}, sheet),
}
-- Facing 1..8 runs clockwise from north.
local DRAWING = {1, 2, 3, 2, 1, 2, 3, 2}
local FLIP_X = {false, false, false, false, false, true, true, true}
local FLIP_Y = {false, false, false, true, true, true, false, false}
local ships = sprites.group({slots = 1})
local facing = makeBounded(1, 1, 8)

function init()
  mode = 1
  screen.main.obj = true
  obj.char_base = sheet.char
end

function frame(t, f)
  if pad.pressed.right then facing = facing % 8 + 1 end
  if pad.pressed.left then facing = (facing + 6) % 8 + 1 end
  ships:draw(SHIP[DRAWING[facing]], 120, 104, {flip_x = FLIP_X[facing], flip_y = FLIP_Y[facing]})
end
```
