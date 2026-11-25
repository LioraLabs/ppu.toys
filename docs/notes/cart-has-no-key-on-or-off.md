---
id: cart-has-no-key-on-or-off
kind: gotcha
title: A cartridge has no kon() or koff(): sfx keys its own voice, and len ends the note
codes: [snes.unsupported]
---
`kon(v)` and `koff(v)` are chip registers of the preview; the cartridge refuses them with
`snes.unsupported`. [[sfx]] already keys the voice off and on for you, so delete the call. To end a note
sooner, give it a length: `len` in seconds on the call, or on the [[bank]] preset. A looping sample stops
after 0.15 s without one ([[looping-sample-stops-after-len]]); a one-shot drum ends with its sample.

```luau
local sound = require("@ppu/sound")
local blip = bank("bell", {vol = 80, len = 0.2})

function init()
  sound.volume(100)
end

function frame(t, f)
  if pad.pressed.a then sfx(blip, 5, "C5") end
  if pad.pressed.b then sfx(blip, 6, "C4", {len = 0.5}) end
end
```
