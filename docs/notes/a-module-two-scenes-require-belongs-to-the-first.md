---
id: a-module-two-scenes-require-belongs-to-the-first
kind: gotcha
title: A module that two scenes require, and that main.luau does not, places its graphics only in the first scene added that requires it
codes: []
fixture: twinstar
---
Placements, timers and songs made while a module loads belong to the scene whose file first required it. When the title requires `art.luau` and so does
the game, the game finds no placements from it: the module loaded once, under the title. Sprite groups are the exception, they draw in both scenes.
To share graphics, require the module from `main.luau` (the program's own, loaded in every scene), and keep the groups several scenes draw
in a module only the scenes require ([[sprite-groups-live-in-groups-luau]]). See [[placements-a-scene-owns]] for the placements themselves.

```luau
-- main.luau
local Shared = require("./shared") -- main requires the art, so every scene loads it
scenes.add(require("./title"))
PLAY = scenes.add(require("./play"))

-- shared.luau
local sheet = dma("ships", {palette = "ships_pal", char = 0x4000, pal = 0})
local SHIP = sprites.art({{size = 16, columns = 1, cells = {0}}}, sheet)
local char = sheet.char
return {SHIP = SHIP, char = char}

-- groups.luau
return {ships = sprites.group()}

-- title.luau
local Shared = require("./shared")
local groups = require("./groups")
local function init()
  mode = 1
  screen.main.obj = true
  obj.char_base = Shared.char
end
local function frame(t, f)
  groups.ships:draw(Shared.SHIP, 128, 104)
  if pad.pressed.start then scenes.go(PLAY) end
end
return {init = init, frame = frame}

-- play.luau
local Shared = require("./shared")
local groups = require("./groups")
local function init()
  mode = 1
  screen.main.obj = true
  obj.char_base = Shared.char
end
local function frame(t, f)
  groups.ships:draw(Shared.SHIP, 100, 80)
end
return {init = init, frame = frame}
```
