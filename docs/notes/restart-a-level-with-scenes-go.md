---
id: restart-a-level-with-scenes-go
kind: recipe
title: A level restarts by scenes.go to its own scene, and its init puts everything back
codes: []
---
When the hero loses a life, the level starts over by switching to itself: [[scenes.go|scenes.go(MEADOW)]]
from MEADOW's own `frame`. The cold boot reloads the scene's art and map, and `init` runs again, so it
must [[scene-init-resets-module-state|reset every value]] the run changed: the hero's place, the pickups
taken, the enemies' patrols. What survives the restart (lives, a score) lives in the
[[scene-handoff-state|shared module table]].

Shared code that decides the way out returns a flag and lets the scene name itself, so one helper
serves every level.

```luau
-- main.luau
MEADOW = scenes.add(require("./meadow"))

-- meadow.luau
local START = {x = 32, y = 160}
local hero = {x = makeBounded(32, 0, 255), y = makeBounded(160, 0, 223)}
local shards = {{x = 80, taken = false}, {x = 160, taken = false}}

-- Shared code would return this flag; the scene names itself.
local function lost(): boolean
  return pad.pressed.select
end

local function init()
  hero.x, hero.y = START.x, START.y
  for _, s in shards do s.taken = false end
end

local function frame(_t: number, _f: number)
  if pad.right then hero.x = math.min(hero.x + 1, 255) end
  for _, s in shards do
    if hero.x == s.x then s.taken = true end
  end
  if lost() then scenes.go(MEADOW) end
end

return {init = init, frame = frame}
```
