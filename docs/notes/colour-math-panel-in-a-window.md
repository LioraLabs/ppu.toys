---
id: colour-math-panel-in-a-window
kind: recipe
title: A dialogue panel is colour math halving the world inside the colour window, opened per line by an hdma() hook
codes: []
---
A panel behind text needs no tiles: subtract black and halve ([[color|color.op = "sub"]],
`color.half = true`) on the world layers and the backdrop, only inside colour window 1
(`win.color.w1 = true`, `color.region = "inside"`). Set that once in `init`, with the window empty
(`lo` past `hi`). While the box is open, an [[hdma]] hook over the panel's lines gives the window its left
and right edges; on the frames it is closed, call no hook and the window stays empty. The text goes on
a layer colour math leaves alone.

```luau
local open = false

function init()
  win.w1.lo, win.w1.hi = 255, 0
  win.color.w1 = true
  color.region = "inside"
  color.addend, color.fixed = "fixed", rgb(0, 0, 0)
  color.op, color.half = "sub", true
  color.on.bg1, color.on.bg2, color.on.backdrop = true, true, true
end

function frame(t, f)
  if pad.pressed.a then open = not open end
  if open then
    hdma(160, 215, function(_y)
      win.w1.lo, win.w1.hi = 16, 239
    end)
  end
end
```
