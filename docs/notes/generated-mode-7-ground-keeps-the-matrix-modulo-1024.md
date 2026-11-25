---
id: generated-mode-7-ground-keeps-the-matrix-modulo-1024
kind: gotcha
title: On a generated Mode 7 ground the matrix centre and scroll take the position modulo 1024, because the plane is 1024 pixels round
codes: []
---
The generated ground can be 32768 pixels round, but the Mode 7 plane is 1024, and the matrix and scroll registers hold 13 bits, so a position past 4095 breaks the picture (a fixed map never gets that far). Keep the position the game and `w:follow` use, and
give the matrix and the scroll the position on the plane: `px = x % 1024`, `m7.cx = px`, `bg[1].scroll.x = px - 128`. Follow with the ground position, as for any [[mode-7-track-streams-within-radius|streamed plane]], and wrap it at the period
([[generated-ground-wraps-at-its-period]]). The plane's window wraps cells round itself, so the picture agrees at every seam.

```luau
local SIZE = 4096 * 8 -- a 4096-cell period: the ground is 32768 px round
local x = makeBounded(4000, 0, 32767)
local y = makeBounded(4000, 0, 32767)

function init()
  mode = 7
  m7.a, m7.d = 1, 1
  screen.main.bg1 = true
end

function frame(t, f)
  if pad.right then x = (x + 4) % SIZE end
  if pad.down then y = (y + 4) % SIZE end
  -- the plane is 1024 px round: the matrix and scroll take the position on it
  local px, py = x % 1024, y % 1024
  m7.cx, m7.cy = px, py
  bg[1].scroll.x, bg[1].scroll.y = px - 128, py - 113
end
```
