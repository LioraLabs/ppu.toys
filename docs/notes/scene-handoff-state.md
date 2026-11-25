---
id: scene-handoff-state
kind: recipe
title: State one scene hands the next lives in a module table that both require
codes: []
---
A [[scene-init-calls-setup|scene switch]] resets the console but not Luau modules: a table returned by
shared.luau is the same table in every scene. Put what crosses a switch there (the last score, the
lives left, the door the hero came through) and give each field its [[makeBounded]] range like any
other [[bound-every-lasting-value|lasting value]]. One scene writes it, the next reads it.

Because the table outlives the switch, the scene that starts a new game puts every field back:
see [[scene-init-resets-module-state]].

```luau
-- main.luau
TITLE = scenes.add(require("./title"))
PLAY = scenes.add(require("./play"))

-- shared.luau
local result = {score = makeBounded(0, 0, 999), best = makeBounded(0, 0, 999)}
return {result = result}

-- play.luau: writes the score, and the best when the clock runs out
local Shared = require("./shared")
local left = makeBounded(600, 0, 600)

local function init()
  Shared.result.score = 0
  left = 600
end

local function frame(_t: number, _f: number)
  if pad.pressed.a then Shared.result.score = math.min(Shared.result.score + 1, 999) end
  left = math.max(left - 1, 0)
  if left == 0 then
    Shared.result.best = math.max(Shared.result.best, Shared.result.score)
    scenes.go(TITLE)
  end
end

return {init = init, frame = frame}

-- title.luau: reads it
local Shared = require("./shared")
local best = makeBounded(0, 0, 999)

local function init()
  best = Shared.result.best
end

local function frame(_t: number, _f: number)
  if pad.pressed.start then scenes.go(PLAY) end
end

return {init = init, frame = frame}
```
