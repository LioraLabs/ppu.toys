---
id: sprite-groups-live-in-groups-luau
kind: rule
title: Groups several files draw live in groups.luau, required only by scene files; a group one file draws is made in that file
codes: []
fixture: title
---
A group's draws count in the scenes whose files draw it. Modules `main.luau` requires load in every scene,
so a group made there (or drawn by a helper there) counts everywhere. The templates' answer: the art stays
in the shared module (`shared.luau`, `art.luau`), the groups more than one file draws go in `groups.luau`,
which only scene files require, and a group that a single file draws is made at the top of that file.
`groups.luau` returns the groups as a table, in the order they stack ([[sprite-groups-stack-in-creation-order|first
made on top]]).

A scene file that is the only drawer keeps its own groups, as here:

```luau
local sheet = dma("sprites", {palette = "sprites_pal", char = 0x4000, pal = 0})
local HERO = sprites.art({{size = 16, columns = 1, cells = {0}}}, sheet)
local DOT = sprites.art({{size = 16, columns = 1, cells = {1}}}, sheet)

-- Only this file draws these, so they are made here.
local heroes = sprites.group()
local dots = sprites.group()

function init()
  mode = 1
  screen.main.obj = true
  obj.char_base = sheet.char
end

function frame(t, f)
  heroes:draw(HERO, 128, 120)
  dots:draw(DOT, 200, 120)
end
```
