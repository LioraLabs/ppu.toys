---
id: hdma-channels-cost-every-frame
kind: concept
title: Each scanline channel takes CPU time on every video frame, in proportion to its rows
codes: [snes.budget]
---
A scanline effect ([[hdma]] or [[bakeHdma]]) runs on its own while the screen draws, but every channel it
uses pauses the processor for a moment each video frame, and longer the more table bytes it reads. One
full-screen channel takes about 4% of a frame, every frame, and every update stretches by that share. Cover
only the rows that change (`hdma(0, 63, ...)`, not the whole screen when the bottom is still) and leave
registers that do not vary by row out of the effect.

```luau
function init()
  mode = 1
  screen.main.bg1 = true
end

function frame(t, f)
  -- Only the top 64 rows sway: a quarter of the channel a full-screen sway would take.
  hdma(0, 63, function(line)
    bg[1].scroll.x = math.floor(math.sin(line / 8 + f / 16) * 4)
  end)
end
```
