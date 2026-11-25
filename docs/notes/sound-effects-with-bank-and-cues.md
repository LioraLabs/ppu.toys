---
id: sound-effects-with-bank-and-cues
kind: recipe
title: Sound effects are bank() presets at file level, named in sound.cues and played by name
codes: []
---
[[bank]] places a built-in sample or one of yours and returns a preset; make it once, at file level.
`sound.cues` names each effect as `{preset, note, priority}` from a literal table, also at file
level, and `cues:play("name")` keys it on the voice the sound driver picks
([[voices-for-songs-and-effects]]); `{preset, voice, note, priority}` names the voice instead. On a cartridge the program must declare `frame` at file level,
since each update's priority resets at its end. [[sfx]] with a preset and a note is the same
call without names. A scene that owns its sounds keeps its cues; several scenes share them through one
module's export.

```luau
local sound = require("@ppu/sound")
local zap = bank("pluck", {vol = 36})
local boom = bank("crash", {vol = 72})
local cues = sound.cues({
  shot = {zap, "C6"},
  boom = {boom, "C3"},
})

function init()
  sound.volume(100)
end

function frame(t, f)
  if pad.pressed.a then cues:play("shot") end
  if pad.pressed.b then cues:play("boom") end
end
```
