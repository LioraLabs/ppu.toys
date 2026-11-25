---
id: draws-modes-0-to-4-and-7
kind: rule
title: The preview draws background modes 0 to 4 and Mode 7 only; pick one of those
codes: []
---
`mode` takes 0 to 7, but the engine draws the tile modes 0 to 4 and Mode 7. Modes 5 and 6 (hi-res),
interlace, overscan and the read-only counter registers are not modelled, so a toy that sets them shows
nothing the hardware would. Mode 1 is the everyday choice: two 16-colour layers and a 4-colour one for
text.

```luau
function init()
  mode = 1
  screen.main.bg1 = true
  screen.main.obj = true
end
```
