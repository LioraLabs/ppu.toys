---
id: read-voice-envx-with-a-literal-index
kind: gotcha
title: voice[i].envx with a computed i reads all eight voices on a cartridge; a literal index reads one
codes: [snes.budget]
---
A cartridge reads a voice's envelope from the sound chip once an update, for the voices the code names.
`voice[3].envx` reads one voice, a small cost. `voice[i].envx` with `i` from a loop or a variable reads all
eight, about an eighth of a frame every update. Read the voices you show with literal indices, and keep the
pulse a visual follows on one voice ([[voice-envelope-drives-visuals]]).

```luau
function init()
  mode = 1
end

function frame(t, f)
  brightness = math.min(15, 4 + voice[0].envx // 8) -- a literal voice: one read
end
```
