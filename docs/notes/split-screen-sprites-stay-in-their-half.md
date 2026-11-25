---
id: split-screen-sprites-stay-in-their-half
kind: recipe
title: sprites.rows(top, bottom) keeps the rest of a frame's draws inside those rows, so a sprite never bleeds into the other half of a split screen
codes: []
fixture: twinstar
---
Sprites share one OAM, so a sprite near a split line would show in both halves. Call [[sprites.rows]] before the draws of one
half: every hardware sprite of the draws after it shows only while all of it lies inside rows `top` up to but not including `bottom`.
The rows are per draw, not per scanline, so a sprite crossing the line disappears whole rather than being cut. The next frame starts with
the whole screen again, so call it every frame.
```luau
local sheet = dma("ships", {palette = "ships_pal", char = 0x4000, pal = 0})
local SHIP = sprites.art({{size = 16, columns = 1, cells = {0}}}, sheet)
local top, bottom = sprites.group(), sprites.group()

function init()
  mode = 1
  screen.main.obj = true
  obj.char_base = sheet.char
end

function frame(t, f)
  sprites.rows(0, 112)
  top:draw(SHIP, 100, 40 + f % 80)
  sprites.rows(112, 224)
  bottom:draw(SHIP, 160, 120 + f % 80)
end
```
