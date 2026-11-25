---
id: mode-7-floor-under-a-sky
kind: recipe
title: A Mode 7 floor under a sky switches mode per line, with window 1 hiding BG1 above the horizon
codes: []
---
Above the horizon a line is [[mode]] 1 (so BG2 can carry text), window 1 spans the whole line and hides
BG1, which is not a plane there, and colour math paints the sky. From the horizon down a line is mode 7,
the window is empty, and the line gets its own [[m7]] matrix: scale `HEIGHT / (y - HORIZON + 1)` makes
far lines stride and near lines creep. Bake it over the heading. Enable the window for BG1 once in
`init`. A view that turns about a car needs more: [[mode-7-perspective-per-heading]]. The templates bake
their own floor like this.

```luau
local HORIZON, HEIGHT = 64, 40

local floor_lines = bakeHdma({heading = {-8, 8}}, 0, 223, function(y, heading)
  if y < HORIZON then
    mode = 1
    win.w1.lo, win.w1.hi = 0, 255
    color.fixed = rgb(10 + y // 4, 8 + y // 8, 40 + y // 2)
  else
    mode = 7
    win.w1.lo, win.w1.hi = 255, 0
    local angle = heading * 3 * math.pi / 180
    local scale = HEIGHT / (y - HORIZON + 1)
    local a, c = math.cos(angle) * scale, math.sin(angle) * scale
    m7.a, m7.b, m7.c, m7.d = a, -c * 128 / (y + 128), c, a * 128 / (y + 128)
    color.fixed = 0
  end
end)

local z = makeBounded(0, 0, 1023)

function init()
  mode = 7
  screen.main.bg1 = true
  win.bg1.w1, win.bg1.main = true, true
  color.on.backdrop, color.op, color.addend = true, "add", "fixed"
end

function frame(t, f)
  z = (z + 1020) % 1024
  m7.cx, m7.cy = 512, z
  bg[1].scroll.x, bg[1].scroll.y = 384, z + 127
  floor_lines(0)
end
```
