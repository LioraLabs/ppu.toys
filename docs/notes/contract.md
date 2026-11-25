---
id: contract
kind: diagnostic
title: A store outside its makeBounded or makeFixed range fails ppu check, because a cartridge stops on the first one
codes: [contract]
---
`main.luau:2: error[contract]: x = 300 outside 16..240 (scene, frame 12, 3×)` is one store the
preview made outside the range a [[makeBounded]] or [[makeFixed]] declares: the file and line of the
store, the value, the range, the scene and first frame it happened (`at load` for the initial value),
and how often. The preview runs on with the wrong value, so `ppu check` lists every violation and then
fails; the Studio shows the same list. A cartridge sizes the value from its range and stops at the first
one ([[snes.halt]]).

Fix the value, not the range alone: clamp or wrap it so every value the code can produce stays inside
(positions that leave the screen, timers, counters), and give the range the honest bounds of what the
game reaches. An `at load` violation is an initial value outside its own range.

```luau
local x = makeBounded(16, 16, 240)

function frame(t, f)
  local step = (pad.right and 2 or 0) - (pad.left and 2 or 0)
  x = math.clamp(x + step, 16, 240)
  bg[1].scroll.x = x
end
```
