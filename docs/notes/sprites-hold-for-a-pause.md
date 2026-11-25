---
id: sprites-hold-for-a-pause
kind: recipe
title: sprites.hold() keeps every sprite on screen for a frame that draws none, such as a pause
codes: []
fixture: twinstar
---
Groups show only what was drawn this frame, so a paused game that skips its updates and draws would
blank the screen. Call [[sprites.hold]] on those frames instead: every group keeps last frame's sprites.
A frame that holds may not also draw, so put the hold and the game's draws in the two branches of one
`if`. Text on a [[hud-is-retained|retained HUD]] stays without it. Dialog boxes that stop the world use the
same branch.

```luau
local sheet = dma("ships", {palette = "ships_pal", char = 0x4000, pal = 0})
local SHIP = sprites.art({{size = 16, columns = 1, cells = {0}}}, sheet)
local ships = sprites.group()
local paused = false
local x = makeBounded(128, 0, 255)

function init()
  mode = 1
  screen.main.obj = true
  obj.char_base = sheet.char
end

function frame(t, f)
  if pad.pressed.start then paused = not paused end
  if paused then
    sprites.hold()
  else
    x = (x + 1) % 256
    ships:draw(SHIP, x, 104)
  end
end
```
