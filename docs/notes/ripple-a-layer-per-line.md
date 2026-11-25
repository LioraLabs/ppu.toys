---
id: ripple-a-layer-per-line
kind: recipe
title: A rippling logo is a per-line scroll.x from a sine, baked over the phase
codes: []
---
Shift one background sideways by a different amount on each line of a band, and the band waves. The
sine is per line, so it belongs in a [[bake-scanline-effects|bake]] over the phase, not in a live
[[hdma]]: a 32-step phase is 32 tables in ROM, and the frame only picks one. Reset the scroll outside
the band, or the last line's shift holds to the bottom of the screen.

```luau
local ripple = bakeHdma({phase = {0, 31}}, 0, 223, function(y, phase)
  if y >= 16 and y < 48 then
    bg[2].scroll.x = math.floor(math.sin((y + phase * 2) * math.pi / 16) * 3)
  else
    bg[2].scroll.x = 0
  end
end)

function frame(t, f)
  ripple(f // 2 % 32)
end
```
