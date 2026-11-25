---
id: a-perspective-floor-loads-with-a-view-and-follows-with-a-heading
kind: concept
title: A perspective Mode 7 floor loads its map with a view of cells ahead and behind the camera and follows with the heading
codes: []
---
A floor tilted per line (a road running to the horizon) sees a long wedge of the map in front of the camera and nothing behind it. Load such a map with `view = {ahead = A, behind = B}` instead of a radius
(`A + B` at most 120, whole numbers): the whole 128x128 plane streams, with its window's centre leading the camera by `(A - B) / 2` cells along the heading. `speed`, a view's alone, is the most pixels a frame on each axis the window keeps up with, 1 to 8 (8
when left out). Follow it with `w:follow(x, y, angle)` every frame and in `init()` after the bind, `angle` the heading in 256ths of a turn, 0 up the plane, growing clockwise; a view world refuses a follow without the angle and any other world one with it.
The map is 128 to 512 cells on each side.

```luau
local ahead, behind = 72, 0 -- world.load("field", {view = {ahead = 72, behind = 0}, speed = 4})
local heading = makeBounded(0, 0, 255)
local lead = makeBounded(0, 0, 60)

function frame(t, f)
  if pad.left then heading = (heading + 254) % 256 end
  if pad.right then heading = (heading + 2) % 256 end
  -- the window's centre leads the camera by (ahead - behind) / 2 cells along the heading
  lead = (ahead - behind) // 2
end
```
