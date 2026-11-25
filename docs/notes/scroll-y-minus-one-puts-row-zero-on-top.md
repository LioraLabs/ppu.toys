---
id: scroll-y-minus-one-puts-row-zero-on-top
kind: gotcha
title: The top screen line shows tilemap row scroll.y + 1, so a background that must start at row 0 sets scroll.y = -1
codes: []
fixture: platformer
---
As on hardware, whose first visible scanline is line 1, a background's top display line shows tilemap row `scroll.y + 1`. A
picture whose first row must sit flush with the top of the screen therefore takes `bg[n].scroll.y = -1`, and a layer left at 0 has
its first row cut off. [[sprites.group|Sprites]] have no such offset: their rows are screen rows.
Scroll values wrap when the map wraps, so a far layer can scroll without a clamp ([[parallax-second-background]]).

```luau
local sky = dma("sky", {palette = "sky_pal", char = 0x3000, map = 0x0800, pal = 0})

function init()
  mode = 1
  bg[1].char_base, bg[1].map_base = sky.char, sky.map
  bg[1].scroll.y = -1 -- row 0 of the map on the top line
  screen.main.bg1 = true
end

function frame(t, f)
  bg[1].scroll.x = f // 2
end
```
