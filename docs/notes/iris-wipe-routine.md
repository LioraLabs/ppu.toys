---
id: iris-wipe-routine
kind: recipe
title: An iris wipe is a routine that closes a baked circular window a step a frame, then switches scene
codes: []
---
Bake the window once over its one input, the radius, with [[bakeHdma]]: each line's window 1 edges are
the circle's chord there, and every layer is masked outside it. A [[routine]] closes it, one radius step a
[[yield]], and its last line is `scenes.go(scene)`; `wipe:start(scene)` asked again while it runs changes
nothing, so a second press cannot start a second wipe. The next scene opens it again a step a frame.
[[fade-a-scene-switch|scenes.go's fade]] is the simpler way between scenes; the iris shows the routine's
shape.

In the snippet the closed iris opens again instead of switching, so it runs on its own.

```luau
local iris_lines = bakeHdma({r = {0, 15}}, 0, 223, function(y, r)
  local dy, reach = y - 112, r * 12
  if math.abs(dy) < reach then
    local half = math.floor(math.sqrt(reach * reach - dy * dy))
    win.w1.lo, win.w1.hi = math.max(0, 128 - half), math.min(255, 128 + half)
  else
    win.w1.lo, win.w1.hi = 255, 0
  end
end)

local radius = makeBounded(15, 0, 15)

local function mask(on: boolean)
  win.bg1.w1, win.obj.w1 = on, on
  win.bg1.invert, win.obj.invert = true, true
  win.bg1.main, win.obj.main = on, on
end

local wipe = routine(function()
  mask(true)
  for r = 14, 0, -1 do
    radius = r
    yield()
  end
  -- scenes.go(scene) here, between scenes
end)

function frame(t, f)
  if pad.pressed.a then wipe:start() end
  if wipe:running() then
    wipe:step()
  elseif radius < 15 then
    radius += 1
    if radius == 15 then mask(false) end
  end
  if radius < 15 then iris_lines(radius) end
end
```
