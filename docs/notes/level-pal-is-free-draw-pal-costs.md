---
id: level-pal-is-free-draw-pal-costs
kind: gotcha
title: A level's pal draws a whole art in another palette for free; a draw's pal rewrites its sprites, so use it for a flash
codes: []
fixture: twinstar
---
Each cell draws in its own palette: the sheet's `pal` plus the palette the importer gave the cell. `pal = n` (0 to 7) in a
level of [[sprites.art]] draws that level in palette n and costs nothing when it draws, so the same cells make a second art:
a shadow, or a player's colours. `pal = n` in a draw does it for one draw, at the cost of rewriting that draw's sprites,
and again on the next draw that leaves it out. Use the draw option for a hit flash and the level's `pal` for colours that
last. For a new set of colours, not just another palette number, see [[sprite-palette-variants]].

```luau
local sheet = dma("ships", {palette = "ships_pal", char = 0x4000, pal = 0})
local PLAIN = sprites.art({{size = 16, columns = 1, cells = {0}}}, sheet)
local SHADOW = sprites.art({{size = 16, columns = 1, cells = {0}, pal = 1}}, sheet)
local ships = sprites.group()

function init()
  mode = 1
  screen.main.obj = true
  obj.char_base = sheet.char
end

function frame(t, f)
  ships:draw(SHADOW, 124, 110)
  ships:draw(PLAIN, 120, 104, {pal = if f % 8 < 2 then 2 else nil})
end
```
