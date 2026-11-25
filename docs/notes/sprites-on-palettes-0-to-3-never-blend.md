---
id: sprites-on-palettes-0-to-3-never-blend
kind: gotcha
title: Colour math only blends sprites drawn with OBJ palettes 4 to 7; palettes 0 to 3 stay opaque
codes: []
fixture: twinstar
---
`color.on.obj = true` turns colour math on for sprites, but the hardware applies it only to sprites whose OBJ
palette is 4, 5, 6 or 7. A sprite on palettes 0 to 3 stays opaque however colour math is set, and the Studio
shows the same. Draw the ghost, the glass or the shadow with `pal = 4` or higher (in the art's level, or in the
draw's options, see [[sprite-palette-variants]]) and keep solid sprites on 0 to 3, so one screen can hold both.

```luau
local _ships = dma("ships", {palette = "ships_pal", char = 0x4000, pal = 0})
local bits = dma("bits", {palette = "bits_pal", char = 0x4800, pal = 4})
local GHOST = sprites.art({{size = 8, columns = 1, cells = {3}}}, bits)
local spirits = sprites.group()

function init()
  mode = 1
  screen.main.obj = true
  obj.char_base = _ships.char
  color.on.obj = true
  color.op = "add"
  color.fixed = rgb(64, 64, 64)
end

function frame(t, f)
  spirits:draw(GHOST, 120, 100, {pal = 4}) -- palette 4: brightened by the fixed colour
end
```
