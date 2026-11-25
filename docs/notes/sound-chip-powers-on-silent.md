---
id: sound-chip-powers-on-silent
kind: gotcha
title: The sound chip powers on and enters every scene at master volume 0, so each scene's setup sets it
codes: []
---
[[dsp.mvol]] starts at 0: sound effects play into silence until it is set. A scene switch is a cold boot,
so it starts at 0 again in every scene. Starting a [[score]] while it is 0 opens it to full by itself;
effects alone do not. Set it in the setup every scene's `init` calls, with `sound.volume` from
`@ppu/sound`, or write `dsp.mvol.l` and `dsp.mvol.r` yourself.

```luau
local sound = require("@ppu/sound")
local cues = sound.cues({start = {bank("pluck", {vol = 60}), 5, "C5"}})

function init()
  sound.volume(100)
  cues:play("start")
end

function frame(t, f)
end
```
