---
id: mode-7-scroll-puts-the-centre-mid-screen
kind: recipe
title: To show plane point (cx, cy) at the middle of the screen, set m7.cx, m7.cy and the scroll to cx - 128, cy - 113
codes: []
fixture: racer
---
The matrix maps a screen pixel back to a plane texel, around the centre: `px = x + scroll.x - cx`,
`py = y + 1 + scroll.y - cy`, then `source = (a·px + b·py + cx, c·px + d·py + cy)`. The `+ 1` is the
hardware's line numbering, the top display line being line 1. So the point at screen (128, 112) is
the centre `(cx, cy)` when `bg[1].scroll.x = cx - 128` and `bg[1].scroll.y = cy - 113`, whatever the
matrix; turning or zooming then happens about the middle of the screen. Smaller matrix entries
enlarge the picture. The plane is 1024 pixels square. Follow a car or a camera by writing its
position into `cx` and `cy` and recomputing the scroll the same way; the matrix is a
[[mode-7-matrix-is-signed-8-8|signed 8.8 number]].

```luau
local _plane = dma("tiles", {palette = "tiles_pal"})
local x, y = makeBounded(512, 0, 1023), makeBounded(512, 0, 1023)

function init()
  mode = 7
  screen.main.bg1 = true
end

function frame(t, f)
  if pad.right then x = math.min(1023, x + 1) end
  if pad.down then y = math.min(1023, y + 1) end
  m7.cx, m7.cy = x, y
  bg[1].scroll.x, bg[1].scroll.y = x - 128, y - 113
end
```
