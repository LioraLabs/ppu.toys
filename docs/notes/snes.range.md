---
id: snes.range
kind: diagnostic
title: A store that can leave a value's bounds is warned before the cartridge stops on it
codes: [snes.range]
---
The warning `This store can give x ..., outside its range lo..hi` means the compiler found a store
to a [[makeBounded]] (or [[makeFixed]]) value that can produce a number outside the bounds you
gave. The cartridge checks the bounds on every store and stops on the first value outside them,
which is the [[snes.halt]] you would meet later in play.

Clamp or wrap the value in the store itself (`math.max`, `math.min`, `%`), or widen the bounds if
larger values are meant. A check on a later line does not help: the store must already be inside.

```luau
local hp = makeBounded(8, 0, 8)

function frame(t, f)
  if pad.pressed.b then
    hp = math.max(hp - 3, 0) -- was hp - 3, which can reach -1
  end
  brightness = 7 + hp
end
```
