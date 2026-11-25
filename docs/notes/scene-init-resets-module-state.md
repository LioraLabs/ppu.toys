---
id: scene-init-resets-module-state
kind: gotcha
title: Module tables outlive a scene switch, so a scene's init sets every value the last run left
codes: []
---
File-level code runs once, at power-on. A scene's locals at file level (the hero's position, a timer,
a module's `game` table) keep whatever the last visit left in them when [[scenes.go]] comes back: a
second game starts with the first game's score and clock at zero, which ends it at once.

So `init` sets every value a fresh run needs, not only the ones you think changed. A level that
[[restart-a-level-with-scenes-go|restarts itself]] relies on this: everything a restart must put back
goes in `init`.

```luau
-- main.luau
PLAY = scenes.add(require("./play"))

-- play.luau
local hero = {x = makeBounded(128, 16, 240), y = makeBounded(120, 32, 208)}
local left = makeBounded(1200, 0, 1200)

local function init()
  hero.x, hero.y = 128, 120
  left = 1200
end

local function frame(_t: number, _f: number)
  if pad.right then hero.x = math.min(hero.x + 1, 240) end
  left = math.max(left - 1, 0)
  if left == 0 then scenes.go(PLAY) end
end

return {init = init, frame = frame}
```
