---
id: doors-from-map-objects
kind: recipe
title: A door is a Tiled box with props to, x and y, leading to another scene or to another place on the same map
codes: []
fixture: adventure
---
Draw each door as a box on the map's object layer with Class `door` and custom properties: `to`, where
it leads, and `x`, `y`, where the feet arrive. Collect the doors from `w.objects` at file level
([[map-objects-by-name-and-class|map objects]]) and test the hero's feet against each box every frame.

There are two kinds. A door to another scene stores the arrival point in the shared game state and
calls [[scenes.go|`scenes.go(CAVE, {fade = 15})`]]; that scene's `init` places the hero there. A door
to another place on the same map (a house's room) fades the screen out, and under the black moves the
hero and the camera and fades back in: the frame's one [[follow-once-a-frame|follow]] then redraws the
whole window, a cut nobody sees.

```luau
local world = require("@ppu/world")
local w = world.load("overworld")
local doors = {}
local ndoors = makeBounded(0, 0, 16)
for _, o in w.objects do
  if o.type == "door" then
    ndoors += 1
    doors[ndoors] = o
  end
end
local START = w:find("start")
local x, y = makeBounded(START.x, 0, 2047), makeBounded(START.y, 0, 2015)
local to_x, to_y = makeBounded(0, 0, 2047), makeBounded(0, 0, 2015)
local fade, dir = makeBounded(15, 0, 15), makeBounded(1, -1, 1)

function frame(t, f)
  fade = math.clamp(fade + dir, 0, 15)
  brightness = fade
  if dir < 0 and fade == 0 then
    x, y, dir = to_x, to_y, 1 -- under the black: move, then fade back in
  end
  for _, d in doors do
    if dir > 0 and x >= d.x and x < d.x + d.w and y >= d.y and y < d.y + d.h then
      to_x, to_y, dir = d.props.x, d.props.y, -1
    end
  end
end
```
