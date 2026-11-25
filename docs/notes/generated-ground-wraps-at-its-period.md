---
id: generated-ground-wraps-at-its-period
kind: gotcha
title: A generated world's ground repeats past its period, so the camera and every position wrap at period * 8 pixels
codes: []
---
`period` is the ground's size in cells on each axis: 256 to 4096 and at least 16 chunks on backgrounds (512 or more with 32-cell templates), and 256, 512, 1024, 2048 or 4096 on the Mode 7 plane. Past it the ground repeats, so a position never leaves
`0` to `period * 8 - 1` pixels: wrap it, `y = (y + dy) % (w.period * 8)` with the period written as a number, and keep a [[bound-every-lasting-value|bounded]] camera at that size. The follow takes a move across the seam as the short move it is, and `w.width` and `w.height`
are the period. Nothing is clamped, because the ground has no edge. Put any state that must stay in range (a score by distance, a depth) in its own counter rather than reading it from the wrapped position.

```luau
local SIZE = 512 * 8 -- the ground is 4096 px round
local y = makeBounded(0, 0, 4095)
local depth = makeBounded(0, 0, 60000)

function frame(t, f)
  y = (y + 6) % SIZE -- falling; the follow reads this across the seam as a short move
  depth = math.min(60000, depth + 6) -- the run's own count does not wrap
end
```
