---
id: two-players-one-steer
kind: recipe
title: Two players share one steer(p, input) called with pad and pad2
codes: []
---
Write the controls once, as a function of a player record and the [[pad|Pad]] that flies it, then call
it with [[pad]] for player one and [[pad2]] for player two. Each player's record is made by one
function too, so both have the same [[bound-every-lasting-value|bounded fields]]. Player two joins on
`pad2.pressed.start`.

```luau
local function make_player(x: number)
  return {on = false, x = makeFixed(x, 12, 244, 16), y = makeFixed(176, 24, 212, 16)}
end
local one = make_player(104)
local two = make_player(152)

local function steer(p: typeof(one), input: Pad)
  local dx = (if input.right then 1 else 0) - (if input.left then 1 else 0)
  local dy = (if input.down then 1 else 0) - (if input.up then 1 else 0)
  p.x = math.clamp(p.x + dx * 2, 12, 244)
  p.y = math.clamp(p.y + dy * 2, 24, 212)
end

function frame(t, f)
  one.on = true
  if pad2.pressed.start then two.on = true end
  if one.on then steer(one, pad) end
  if two.on then steer(two, pad2) end
end
```
