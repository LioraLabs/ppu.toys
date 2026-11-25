---
id: voice-envelope-drives-visuals
kind: recipe
title: voice[n].envx is a read-only level from 0 to 127 that a cartridge lets a picture follow
codes: []
---
`voice[n].envx` is the voice's running envelope: it climbs when a note starts and falls as it dies, so
reading it makes a light, a scroll or a palette follow the sound. It is read-only (a store is refused
with a message saying so). Key the sound with [[sfx]] on one of
the effect voices ([[voices-for-songs-and-effects]]) and read that voice: here a flash on every
shot, brightness 8 at rest.

```luau
local sound = require("@ppu/sound")
local zap = bank("pluck", {vol = 80})

function init()
  sound.volume(100)
end

function frame(t, f)
  if pad.pressed.a then sfx(zap, 5, "C5") end
  brightness = 8 + voice[5].envx // 20
end
```
