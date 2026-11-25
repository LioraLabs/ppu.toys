---
id: snes.capacity
kind: diagnostic
title: A list built empty needs a provable size: check its length before each append
codes: [snes.capacity]
---
`error[snes.capacity]` means a table starts empty (`{}`) and nothing in the program bounds how many elements it can
hold, so the cartridge cannot reserve its work RAM. A cartridge sizes each list from the appends the code guards: an
append written as `if #list < N then list[#list + 1] = v end` (or a `table.insert` inside that guard) holds at most N.

Write every append behind such a guard, with N the most the game ever needs at once. A list of numbers that has a
fixed length from the start is simpler still: build it at that length, `local heights = {0, 0, 0, 0}`.

```luau
local drops: {{x: number, y: number}} = {}

function frame(t, f)
  if pad.a and #drops < 16 then
    drops[#drops + 1] = {x = makeBounded(f % 256, 0, 255), y = makeBounded(0, 0, 223)}
  end
  for _, d in drops do
    d.y = (d.y + 1) % 224
  end
end
```
