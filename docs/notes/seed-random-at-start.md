---
id: seed-random-at-start
kind: recipe
title: Seed math.random with the frame Start was pressed, or every power-on plays the same game
codes: []
---
`math.random` is one 16-bit generator, the same in the preview and on a cartridge, and it starts from
the same seed at every power-on: enemies that wander by `math.random` walk the same paths every game.
Call `math.randomseed(f)` when the player presses Start: the frame count there differs each time, so
each game differs. Always pass `math.random` its range (`math.random(1, 6)`): a cartridge refuses it
without arguments.

```luau
local dot = makeBounded(1, 1, 6)

function frame(t, f)
  if pad.pressed.start then
    math.randomseed(f)
  end
  if pad.pressed.a then
    dot = math.random(1, 6)
  end
end
```
