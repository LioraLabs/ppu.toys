---
id: sprite-priority-under-overhead-layer
kind: recipe
title: A sprite at prio 2 walks over the ground and under an overhead layer bound at priority 1
codes: []
fixture: adventure
---
For tree crowns and roofs the hero walks under, draw the world in two tile layers: the ground on one
background at tile priority 0, the overhead layer on another bound with `priority = 1`. Draw the hero with
`prio = 2`: in Mode 1 that puts sprites above priority-0 tiles of both layers and below priority-1 tiles. Every
sprite that walks among the trees takes the same `prio`. Text that must cover the sprites too goes on bg3
([[text-in-front-of-the-world-on-bg3|text in front of the world]]). A draw's `prio` is 3, in front of every background, unless the draw gives another.

```luau
local world = require("@ppu/world")
local _tiles = dma("tiles", {palette = "tiles_pal", char = 0x1000, pal = 32})
local sheet = dma("hero", {palette = "hero_pal", char = 0x6000, pal = 0})
local HERO = sprites.art({{size = 32, columns = 1, cells = {0}}}, sheet)
local heroes = sprites.group({slots = 1})
local w = world.load("overworld")
local START = w:find("start")

function init()
  mode = 1
  obj.char_base = sheet.char
  w:bind("ground", bg[2], {map = 0x0000})
  w:bind("overhead", bg[1], {map = 0x0800, priority = 1})
  w:follow(START.x - 128, START.y - 112)
  screen.main.bg1, screen.main.bg2, screen.main.obj = true, true, true
end

function frame(t, f)
  heroes:draw(HERO, 128, 103, {prio = 2})
end
```
