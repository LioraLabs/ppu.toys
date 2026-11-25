---
id: set-the-seed-before-bind-and-keep-pick-pure
kind: gotcha
title: A generated world's pick must answer the same for the same chunk, so the seed is set before w:bind and never changed while the world is bound
codes: []
---
`pick(cx, cy)` may read `cx`, `cy`, [[turn-tables-with-lookup|lookup tables]] and values that hold still during the run, such as the seed: the library calls it once per chunk that comes into the window, and `w:tile`
asks the same pick again later. Set the seed in `init()` before `w:bind`, or before [[scenes.go]] enters the scene that binds the world; the bind picks every chunk again. A seed changed later leaves the chunks already on the plane as they were and
changes the ones that load next, so the ground no longer agrees with itself. `math.hash(seed, x, y)` is a number from 0 to 255 for a seed (0 to 65535) and two whole numbers, and is the same in the Studio and on a cartridge, so one seed is one land everywhere (`world.hash` is its old name). A pick that returns a number that is no template stops the follow, naming the chunk.
See [[seed-random-at-start]] for choosing the seed.

```luau
local seed = makeBounded(0, 0, 9999)
local template = makeBounded(0, 0, 3)

local function pick(cx: number, cy: number): number
  return math.hash(seed, cx, cy) % 4
end

function init()
  seed = 1225 -- before the world binds: the bind picks every chunk with it
end

function frame(t, f)
  template = pick(f % 32, 5)
end
```
