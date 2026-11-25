---
id: mode-7-wrap-decides-beyond-the-plane
kind: gotcha
title: m7.wrap chooses what shows past the plane's edge, and a smaller image sits in the top-left, not repeated
codes: []
---
The plane is 1024 x 1024 pixels. `m7.wrap` says what a screen pixel that samples outside it shows: `0`
or `1` wraps round the plane, `2` is transparent (the backdrop or a layer behind shows), `3` is the
plane's tile zero. An image smaller than the plane is placed at its top-left with the rest of the map
left as tile zero, and wrapping repeats the whole 1024 plane, not the small image: to tile a pattern
across the ground, repeat it in the source PNG itself (repeated tiles cost nothing, only 256 distinct
tiles are allowed), and for a centred emblem keep the small image and point the centre inside it.

```luau
function init()
  mode = 7
  screen.main.bg1 = true
end

function frame(t, f)
  m7.a, m7.b, m7.c, m7.d = 1, 0, 0, 1
  m7.wrap = 2 -- an island in the backdrop
end
```
