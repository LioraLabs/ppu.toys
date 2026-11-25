---
id: cue-priority-one-key-on-per-update
kind: recipe
title: A cue's priority keys only the most important sound of an update, so two ships firing make one shot
codes: []
---
A key-on costs a cartridge time, and two identical sounds in one update are heard as one. Give cues a
last number, a priority from 1 to 255: in each update a cue plays only above the highest priority
already played in it, so the first shot keys and a second shot is skipped, while a louder event (a
blast, a hurt) still gets through. A cue without a priority always plays. See
[[sound-effects-with-bank-and-cues]].

```luau
local sound = require("@ppu/sound")
local zap = bank("pluck", {vol = 36})
local thud = bank("tom", {vol = 96})
local cues = sound.cues({
  shot = {zap, "C6", 1},
  boom = {thud, "C3", 3},
  hurt = {thud, "C2", 4},
})

function init()
  sound.volume(100)
end

function frame(t, f)
  if pad.a then cues:play("shot") end
  if pad2.a then cues:play("shot") end
  if pad.pressed.b then cues:play("hurt") end
end
```
