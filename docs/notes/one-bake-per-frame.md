---
id: one-bake-per-frame
kind: rule
title: A scene's bakes live in one file and share one schedule, so each frame calls exactly one of them
codes: []
---
A cartridge merges every [[bakeHdma]] of a scene into one scanline schedule: each bake is one branch of
it, and the call a frame makes picks the branch. So keep a scene's bakes in one file (a program without
[[scenes]] keeps all of them in one file), and call one bake per frame. When a transition must draw over
the scene's own effect, give the transition its own bake that repeats what it needs of the scene's (the
shared sky through a [[lookup]]), and call that one instead while it runs: Twinstar's iris works this way.
The refusal `Keep every bakeHdma of a scene in one file` names the second file.

```luau
local sky = lookup({y = {0, 223}}, function(y) return rgb(8, 6, 40 + y // 6) end)

local scene_lines = bakeHdma({phase = {0, 31}}, 0, 223, function(y, phase)
  color.fixed = sky(y)
  bg[1].scroll.x = (y + phase) % 4
end)

local iris_lines = bakeHdma({r = {0, 15}}, 0, 223, function(y, r)
  color.fixed = sky(y)
  local dy, reach = y - 112, r * 12
  if math.abs(dy) < reach then
    local half = math.floor(math.sqrt(reach * reach - dy * dy))
    win.w1.lo, win.w1.hi = math.max(0, 128 - half), math.min(255, 128 + half)
  else
    win.w1.lo, win.w1.hi = 255, 0
  end
end)

local radius = makeBounded(15, 0, 15)

function init()
  color.on.backdrop, color.op, color.addend = true, "add", "fixed"
end

function frame(t, f)
  if pad.pressed.a then radius = 0 elseif radius < 15 then radius += 1 end
  if radius < 15 then iris_lines(radius) else scene_lines(f % 32) end
end
```
