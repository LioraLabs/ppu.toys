---
id: placements-a-scene-owns
kind: concept
title: Placements in modules main.luau requires load in every scene; a scene's own dma() owns its VRAM, and scenes may reuse the same addresses
codes: []
fixture: adventure
---
A file-level [[dma]] in a module that main.luau requires (shared.luau: the font, the hero's sheet)
belongs to the whole program: a cartridge loads it in every scene. A `dma` at file level in a scene's
own file loads only when that scene starts.

Since a [[scene-init-calls-setup|switch clears VRAM]], two scenes never meet, so each can place its own
art at the same addresses: the overworld's beetles and moths and the cave's bat both sit at 0x7000. Shared placements must keep clear of those addresses: write the map down
[[vram-map-comment|at the top of shared.luau]].

```luau
-- main.luau
local _Shared = require("./shared")

OVERWORLD = scenes.add(require("./overworld"))
CAVE = scenes.add(require("./cave"))

-- shared.luau: required by main.luau, so in every scene
local font = dma("font", {palette = "font_pal", char = 0x5000, pal = 28})
local hero_sheet = dma("hero", {palette = "hero_pal", char = 0x6000, pal = 0})
return {font = font, hero_sheet = hero_sheet}

-- overworld.luau: its own placement, loaded only in this scene
local _Shared = require("./shared")
local enemy_sheet = dma("enemies", {palette = "enemies_pal", char = 0x7000, pal = 2})
local _BEETLE = sprites.art({{size = 16, columns = 1, cells = {0}}}, enemy_sheet)

local function frame(_t: number, _f: number)
  if pad.pressed.start then scenes.go(CAVE) end
end

return {init = function() end, frame = frame}

-- cave.luau: the same address, a different scene
local _Shared = require("./shared")
local bat_sheet = dma("bat", {palette = "bat_pal", char = 0x7000, pal = 2})
local _BAT = sprites.art({{size = 16, columns = 1, cells = {0}}}, bat_sheet)

local function frame(_t: number, _f: number)
  if pad.pressed.start then scenes.go(OVERWORLD) end
end

return {init = function() end, frame = frame}
```
