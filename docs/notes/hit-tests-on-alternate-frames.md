---
id: hit-tests-on-alternate-frames
kind: recipe
title: Test hits on alternate frames, half the actors each frame, when nothing can cross a hit box in one skipped frame
codes: []
---
Box tests between every enemy and every shot are the busiest loop in a shooter. Each actor still moves
every frame, but only half of them are tested: a toggle flips per actor, and an actor is tested when the
toggle matches `f % 2`. Every actor is then tested every other frame, at half the cost per frame. It is
safe only when the fastest thing moves less in a frame than the box is wide (a shot of 6 pixels a frame
against a 24-pixel box): check that for your speeds. Actors are [[actors-that-come-and-go-are-a-fixed-list|fixed lists of records]].

```luau
local xs = {40, 80, 120, 160, 200}
local hits = makeBounded(0, 0, 999)
local shot_x = makeBounded(0, 0, 255)

function frame(t, f)
  shot_x = (shot_x + 6) % 256
  local turn = makeBounded(0, 0, 1)
  for _, x in xs do
    turn = 1 - turn
    if turn == f % 2 and math.abs(shot_x - x) < 12 then
      hits = math.min(999, hits + 1)
    end
  end
end
```
