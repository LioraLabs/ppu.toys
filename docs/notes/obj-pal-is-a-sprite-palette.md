---
id: obj-pal-is-a-sprite-palette
kind: gotcha
title: For a sprite sheet, dma's pal is a sprite palette number 0 to 7, not a CGRAM entry
codes: []
fixture: twinstar
---
For `bg`, `sheet` and palette sources, a placement's `pal` is an absolute CGRAM entry (0 to 255). For
an `obj` source it is a sprite palette, 0 to 7: palette `p` starts at CGRAM `128 + p * 16`, and
anything past 7 is refused. So `pal = 6` on a sprite sheet writes CGRAM 224 to 239, while `pal = 6`
on a background writes from entry 6. The returned `pal` and `next_pal` count the same way, and
[[sprites.art]] adds the sheet's palette to each cell's for you. Keep sprite palettes clear of a
256-colour Mode 7 palette, which claims entries from 1 up to its last colour.

```luau
local ships = dma("ships", {palette = "ships_pal", char = 0x4000, pal = 0}) -- CGRAM 128..143
local bits = dma("bits", {palette = "bits_pal", char = 0x4800, pal = 6})    -- CGRAM 224..239
local shot = sprites.art({{size = 8, columns = 1, cells = {0}}}, bits)
local shots = sprites.group()

function init()
  obj.char_base = ships.char
  screen.main.obj = true
end

function frame(t, f)
  shots:draw(shot, 120, 104)
end
```
