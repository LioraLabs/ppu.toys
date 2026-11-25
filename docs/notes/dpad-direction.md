---
id: dpad-direction
kind: recipe
title: The d-pad becomes a direction dx, dy of -1, 0 or 1, and a diagonal moves slower on each axis
codes: []
---
Turn the four [[pad]] directions into two numbers, each -1, 0 or 1: `dx` right minus left, `dy` down
minus up. Opposite buttons cancel out. Multiply by a speed to move.

Held diagonally, both axes move at once, so the hero travels about 1.41 times as far a frame. Scale each
axis down on a diagonal (about speed / 1.41) so every direction feels the same. A table indexed by
`(dy + 1) * 3 + dx + 2` turns the pair into one of eight facings.

```luau
local WALK = 1.5
local DIAGONAL = 1.125 -- about WALK / 1.41

local p = {x = makeFixed(128, 0, 255, 8), y = makeFixed(112, 0, 223, 8)}

function frame(t, f)
  local dx = (if pad.right then 1 else 0) - (if pad.left then 1 else 0)
  local dy = (if pad.down then 1 else 0) - (if pad.up then 1 else 0)
  local speed = if dx ~= 0 and dy ~= 0 then DIAGONAL else WALK
  p.x = math.clamp(p.x + dx * speed, 0, 255)
  p.y = math.clamp(p.y + dy * speed, 0, 223)
end
```
