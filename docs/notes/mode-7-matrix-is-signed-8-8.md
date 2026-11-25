---
id: mode-7-matrix-is-signed-8-8
kind: gotcha
title: Mode 7 matrix entries are signed 8.8 fixed point, so each must stay within ±127.99 and is kept to 1/256
codes: []
---
You write [[m7]] `a`, `b`, `c` and `d` as ordinary numbers, but the chip holds each in 16 bits: 8 integer
bits with the sign and 8 of fraction. A value past ±127.99 does not fit, and anything finer than 1/256 is
rounded. Perspective floors are where this bites: the scale `HEIGHT / n` grows without limit as `n` nears
the vanishing line. Start the ground some lines below it (an offset like `FAR` in
[[mode-7-perspective-per-heading]]) so the largest entry is small; check the worst line by hand when you
change the camera.

```luau
local HORIZON, HEIGHT, FAR = 80, 48, 21  -- the largest scale is HEIGHT / FAR, about 2.3

local floor_lines = bakeHdma({}, 0, 223, function(y)
  if y >= HORIZON then
    mode = 7
    local scale = HEIGHT / (y - HORIZON + FAR)
    m7.a, m7.b, m7.c, m7.d = scale, 0, 0, scale
  else
    mode = 1
  end
end)

function frame(t, f)
  floor_lines()
end
```
