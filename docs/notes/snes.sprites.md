---
id: snes.sprites
kind: diagnostic
title: A cartridge plans sprites from art cut at file level and groups drawn in a global frame(t, f)
codes: [snes.sprites]
fixture: title
---
A cartridge works out where every sprite goes when it builds, from the [[sprites.art]] cut at file
level and the [[sprites.group]] draws in `frame`. `error[snes.sprites]` names what it could not
plan: `sprites.draw` needs `frame(t, f)` declared as a function at file level, because the frame's
sprites are assigned at its end; an animation drawn without a key (whose clock it plays on) or
`{at = frames}`; or more art cells, flipped copies included, than the library's keys hold. The preview
refuses many of the same mistakes with the same code and stops on the same line: fix what it names.

Cut art once with `sprites.art`, make each group once, and draw with `group:draw` every frame. A
shared helper can take the group as an argument (`local function hearts(g) ... g:draw(...) end`) when
every call passes a group itself: one a file made with `sprites.group`, or one a module returns.

```luau
local sheet = dma("sprites", {palette = "sprites_pal", char = 0x4000, pal = 0})
local hero = sprites.art({{size = 16, columns = 1, cells = {0}}}, sheet)
local heroes = sprites.group()

function init()
  obj.char_base = sheet.char
  screen.main.obj = true
end

function frame(t, f)
  heroes:draw(hero, 120, 104)
end
```
