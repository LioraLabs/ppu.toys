---
id: a-view-world-always-loads-the-near-square-and-catches-up-the-far-rows
kind: gotcha
title: A view world keeps a near square exact at any turn rate, and its far rows catch up within a number of follows that a long view can push past the limit
codes: []
---
With `d = ceil(|A - B| / 2)` the near square, `w.near = 64 - d` cells either side of the camera's cell on both axes, is always loaded and exact, so keep the camera that far in from the map's edges (`x` and `y` from `w.near * 8` to `(width - w.near) * 8 + 7`);
a move past the view's `speed` redraws the whole window. The far rows chase the heading, at most 2 plane rows and 2 columns a follow, and catch up within `w.catch_up = ceil((16 * d + 7) / (16 - speed))` follows once the heading settles. A view whose catch-up would pass 60 follows
(a second) is refused when the map loads: shorten `ahead`, lengthen `behind` or lower `speed`. Examples at speed 4: ahead 48 with behind 48 is near 64, catch-up 1; ahead 72 with behind 0 is near 28, catch-up 49; ahead 104 with behind 8 is refused at 65.
A view is also refused with a fractional or negative `ahead` or `behind`, any other key in `view`, a `radius` beside it, or on a map with no Mode 7 layer or one that fits the plane whole. See [[heavy-worlds-target-30-hz]] for a world that costs more than a frame.

```luau
local ahead, behind, speed = 72, 0, 4
local near = makeBounded(0, 0, 64)

function init()
  local d = (math.abs(ahead - behind) + 1) // 2
  near = 64 - d -- 28: the camera stays 28 cells in from the map's edges
  local catch_up = (16 * d + 7 + (16 - speed) - 1) // (16 - speed)
  if catch_up > 60 then error("this view's far rows lag more than a second") end
end

function frame(t, f)
end
```
