---
id: enemy-think-move-bump
kind: recipe
title: An enemy is a state and a timer; think() sets its velocity, the scene moves it against the map and calls bump()
codes: []
---
Give each enemy a `state` (walk, tell, dash, rest, hurt...) and a `timer` of frames left in it, both
bounded. `think(e)` runs once a frame: it counts the timer down, moves to the next state when it ends,
and sets the speed `vx, vy`; it never touches the map. The scene, which owns the map, moves the enemy by
that speed one axis at a time and calls `bump(e)` when a tile stopped it, so a wanderer turns and a dash
ends. The feel lives in named constants (pixels and frames) at the top. Enemies come from a
[[map-placed-actors-are-record-lists|map list]] and think only [[only-update-actors-near-the-screen|near the
screen]].

```luau
local WALK, DASH, REST = 0, 1, 2
local DASH_TIME, REST_TIME = 60, 40
local e = {x = makeFixed(64, 0, 255, 8), vx = makeFixed(0, -3, 3, 8),
  state = makeBounded(WALK, 0, 2), timer = makeBounded(0, 0, 127)}

local function think(e)
  if e.timer > 0 then e.timer -= 1 end
  if e.state == WALK then
    e.vx = 0.5
    if e.timer == 0 then e.state, e.timer = DASH, DASH_TIME end
  elseif e.state == DASH then
    e.vx = 3
    if e.timer == 0 then e.state, e.timer = REST, REST_TIME end
  elseif e.timer == 0 then
    e.state, e.timer = WALK, 30
  end
  if e.state == REST then e.vx = 0 end
end

-- A wall stopped it: a dash ends early.
local function bump(e)
  if e.state == DASH then e.state, e.timer, e.vx = REST, REST_TIME, 0 end
end

function frame(t, f)
  think(e)
  local nx = math.clamp(e.x + e.vx, 16, 240)
  if nx ~= e.x + e.vx then bump(e) end
  e.x = nx
end
```
