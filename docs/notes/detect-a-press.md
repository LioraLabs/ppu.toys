---
id: detect-a-press
kind: gotcha
title: pad.pressed is true only on the update a button goes down, and a button held across a scene switch is not a press
codes: []
---
[[pad|pad.a]] is true while A is held: a menu that acts on it fires every frame. Act on
`pad.pressed.a` (it went down this update) or `pad.released.a` (it came up) for one action per press.

The first update of the program and of each scene has no edges: a Start still held from the title is
not a press in the next scene, so one press never skips two screens. No `was_start` bookkeeping is
needed. [[pad2]] has the same fields.

```luau
local hits = makeBounded(0, 0, 99)

function frame(t, f)
  if pad.pressed.a then
    hits = math.min(99, hits + 1)
  end
  brightness = if pad.b then 8 else 15
end
```
