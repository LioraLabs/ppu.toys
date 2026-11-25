---
id: flip-mirrors-the-whole-art
kind: gotcha
title: flip_x and flip_y mirror the whole art around the drawn point, but 16x32 and 32x64 sprites flip vertically as two squares
codes: []
fixture: twinstar
---
`flip_x` and `flip_y` in a draw mirror every sprite of the art around the drawn point and flip each one, so a large
art flips as a picture; an 8x8 sheet's cell that the importer stored flipped flips back. One drawing then gives its
mirror images, as in [[eight-facings-from-three-drawings]]. The exception is the rectangular pairs (16x32 and 32x64):
the hardware flips such a sprite vertically as two squares, each flipped in place, so art built from pair 6 or 7 and drawn with `flip_y` does not
mirror. Keep art that needs `flip_y` in square sizes.

```luau
local sheet = dma("ships", {palette = "ships_pal", char = 0x4000, pal = 0})
local SHIP = sprites.art({{size = 16, columns = 1, cells = {0}}}, sheet)
local ships = sprites.group()

function init()
  mode = 1
  screen.main.obj = true
  obj.char_base = sheet.char
end

function frame(t, f)
  ships:draw(SHIP, 100, 104)
  ships:draw(SHIP, 140, 104, {flip_x = true, flip_y = f // 30 % 2 == 1})
end
```
