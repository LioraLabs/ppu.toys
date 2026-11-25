---
id: rgb-of-a-variable-costs-a-twentieth-of-a-frame
kind: concept
title: rgb() of literals is free on a cartridge, but rgb() of a variable costs about a twentieth of a frame a call
codes: [snes.budget]
---
`rgb(255, 128, 0)` is worked out when the cartridge is built. With any argument a variable, it is a run-time
conversion of about 5% of a 60 Hz frame a call, so a fade that recomputes eight colours a frame spends near
half of it. Build a fade's colours once: as a [[lookup]] over the fade step returning the packed colour, or in
a loop over constants only ([[counted-loop-math-on-the-counter-becomes-a-rom-table]]), and write the stored
value to `cgram[]`.

```luau
local level = makeBounded(0, 0, 15)
local FADE = lookup({s = {0, 15}}, function(s)
  return rgb(s * 16, s * 8, 0)
end)

function frame(t, f)
  level = f // 4 % 16
  cgram[1] = FADE(level) -- a ROM read, not a conversion
end
```
