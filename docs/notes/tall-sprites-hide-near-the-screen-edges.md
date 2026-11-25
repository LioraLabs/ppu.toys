---
id: tall-sprites-hide-near-the-screen-edges
kind: gotcha
title: A 64-pixel-tall sprite shows only with its top between y = -32 and y = 192, because the hardware wraps its rows to the top
codes: []
fixture: twinstar
---
The PPU wraps a sprite's rows past scanline 255 back to the top of the screen. The library hides a 64-pixel-tall sprite
where it would wrap onto the picture, so its top must lie between -32 and 192; a 32-pixel or smaller sprite hides only when it is wholly
off screen. A tall boss or a 64x64 art that scrolls in from the bottom edge therefore pops in at y = 192 rather than sliding in. Build tall art from 32-pixel
sprites (a 32x64 cell on the 32 and 64 pair is the same wrap), or keep it away from the bottom edge, when it must slide in smoothly.

```luau
local sheet = dma("ships", {palette = "ships_pal", char = 0x4000, pal = 0})
-- 16x16 cells with an 8 and 16 pair draw below the screen edge as usual: only 64-tall sprites wrap.
local SHIP = sprites.art({{size = 16, columns = 1, cells = {0}}}, sheet)
local ships = sprites.group()

function init()
  mode = 1
  screen.main.obj = true
  obj.char_base = sheet.char
end

function frame(t, f)
  local y = 224 - f % 240
  ships:draw(SHIP, 128, math.max(y, -8))
end
```
