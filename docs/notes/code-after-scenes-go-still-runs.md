---
id: code-after-scenes-go-still-runs
kind: gotcha
title: scenes.go only asks for a switch, so the update that calls it finishes as written and the next update enters the scene
codes: []
---
[[scenes.go]] does not return early. The rest of that update runs, draws included, and only the next update cold-boots into the new scene. Code after the call
that moves the hero, spends a life or plays a sound therefore still happens, and happens to the scene being left: put `scenes.go` last, or `return` after
it. The old scene's draws are never seen again, so skip them. Under a `{fade = n}` the old scene keeps running for the whole fade-out, and a
second `scenes.go` while the fade runs changes nothing ([[fade-a-scene-switch]]).

```luau
-- main.luau
PLAY = scenes.add(require("./play"))

-- play.luau
local lives = makeBounded(3, 0, 3)

local function frame(t, f)
  if pad.pressed.a then lives = math.max(0, lives - 1) end
  if lives == 0 then
    scenes.go(PLAY)
    return -- nothing after the switch should run
  end
end

return {frame = frame}
```
