---
id: scene-preview
kind: recipe
title: A scene's preview() stages it for the Studio's Scene view and never runs on power-on or scenes.go
codes: []
---
A scene may return `preview` beside `init` and `frame`. The Studio runs it when its scene switcher
enters the scene, on Restart scene, and when an edit reloads the chosen scene: after file level and
every [[scenes.add]], before `init`. Use it to set the [[scene-handoff-state|hand-off state]] the scene
expects from the one before it (the door the hero came through, a level number), so the scene opens
in the Studio as it would mid-game.

Power-on and [[scenes.go]] never run it, so it cannot change how the game plays; nothing a game needs
belongs there.

```luau
-- main.luau
CAVE = scenes.add(require("./cave"))

-- shared.luau
local game = {door_x = makeBounded(0, 0, 255), door_y = makeBounded(0, 0, 223)}
return {game = game}

-- cave.luau
local Shared = require("./shared")
local game = Shared.game
local hero = {x = makeBounded(0, 0, 255), y = makeBounded(0, 0, 223)}

local function preview()
  game.door_x, game.door_y = 128, 200
end

local function init()
  hero.x, hero.y = game.door_x, game.door_y
end

local function frame(_t: number, _f: number) end

return {init = init, frame = frame, preview = preview}
```
