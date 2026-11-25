---
id: sky-gradient-with-fixed-colour
kind: recipe
title: A sky gradient is colour math adding a per-line fixed colour to a black backdrop
codes: []
---
Turn colour math on for the backdrop once in `init` ([[color|color.on.backdrop]], `color.op = "add"`,
`color.addend = "fixed"`), leave the backdrop black, and set [[color.fixed]] per scanline. The gradient
costs no tiles and no CGRAM writes. Put the colour per line in a [[lookup]] so every bake that needs the
sky shares one table, and [[bake-scanline-effects|bake]] the effect over its one input: here a red flush
while the player is hurt. [[rgb]] takes 0..255 channels.

```luau
local sky = lookup({y = {0, 223}}, function(y)
  return rgb(8 + y // 14, 6 + y // 28, 40 + y // 6)
end)

local lines = bakeHdma({pulse = {0, 7}}, 0, 223, function(y, pulse)
  color.fixed = sky(y) + rgb(pulse * 12, 0, 0)
end)

local hurt = makeBounded(0, 0, 7)

function init()
  color.on.backdrop, color.op, color.addend = true, "add", "fixed"
end

function frame(t, f)
  if pad.pressed.a then hurt = 7 elseif hurt > 0 then hurt -= 1 end
  lines(hurt)
end
```
