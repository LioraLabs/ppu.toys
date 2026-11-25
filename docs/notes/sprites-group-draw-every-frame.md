---
id: sprites-group-draw-every-frame
kind: recipe
title: Art is cut once at file level, one sprites.art per cell, and a group draws it every frame
codes: []
fixture: title
---
A sprite is three things made at file level and one call a frame. [[dma|dma()]] places the sheet;
[[sprites.art]] cuts one piece of art from its cells (one call per drawing, so a cartridge makes each art
from one call); a helper whose body is `return sprites.art(...)` (or an animation of them) is that call,
spelled once, when every call of the helper is at file level with constant arguments: `local function
cell(n) return sprites.art({{size = 16, columns = 1, cells = {n}}}, sheet) end`, then `local walk =
{cell(0), cell(1)}`. [[sprites.group]] makes the group that shows it. Then `frame` calls `group:draw(art, x, y)`
for every sprite on screen, every frame: a sprite not drawn (or kept, see [[keyed-draws-and-keep|keyed
draws]]) this frame is gone, so there is nothing to hide or free. Never write `obj[i]` by hand: the group
assigns the hardware slots, and a cartridge builds the same draws.

```luau
local sheet = dma("sprites", {palette = "sprites_pal", char = 0x4000, pal = 0})
local HERO = sprites.art({{size = 16, columns = 1, cells = {0}}}, sheet)
local heroes = sprites.group()
local x = makeBounded(128, 16, 240)

function init()
  mode = 1
  screen.main.obj = true
  obj.char_base = sheet.char
end

function frame(t, f)
  local dx = (if pad.right then 2 else 0) - (if pad.left then 2 else 0)
  x = math.clamp(x + dx, 16, 240)
  heroes:draw(HERO, x, 120)
end
```
