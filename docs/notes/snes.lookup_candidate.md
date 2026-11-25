---
id: snes.lookup_candidate
kind: diagnostic
title: A pure helper that divides or calls sin, cos, sqrt or ^ can be a ROM table built with lookup()
codes: [snes.lookup_candidate]
---
The warning `Pure calculation ... contains division, trigonometry, square root or power` means a helper computes, from
its parameters alone, maths a 65816 does slowly. A cartridge can compute it ahead of time for every input and read
the answer from ROM. Wrap it in [[lookup|lookup()]] with each parameter's honest range (and a step when the range is
wide), and call the result as before. The table costs ROM in proportion to the grid's points; the check reports it.

A value outside the grid is rounded to the nearest point, so clamp or wrap the inputs. Keep the helper as it is when its
inputs have no small range.

```luau
-- Was: local function aim(dx, dy) local l = math.sqrt(dx * dx + dy * dy) ... end
local AIM = lookup({dx = {-8, 8}, dy = {-7, 7}}, function(dx, dy)
  local length = math.sqrt(dx * dx + dy * dy)
  if length == 0 then return 0, 1.5 end
  return dx / length * 1.5, dy / length * 1.5
end)

local vx = makeFixed(0, -2, 2, 16)
local vy = makeFixed(0, -2, 2, 16)

function frame(t, f)
  vx, vy = AIM(f % 17 - 8, 3)
end
```
