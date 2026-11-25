---
id: snes.bank7e
kind: diagnostic
title: The top of work RAM bank $7E holds the large arrays and the sprite groups' tables, and a bigger cartridge does not add to it
codes: [snes.bank7e]
---
`error[snes.bank7e]` means the arrays too big for low work RAM, together with the sprites library's tables, claim more
of the top of bank $7E than it has. The message lists each claim and its bytes. It is work RAM, fixed by the console,
so cartridge size does not help: the claims must shrink.

Cut the biggest one the message names. For arrays: lower capacities, tighter [[makeBounded]] ranges (a 0..255 element
takes one byte, a wider one two), and data that never changes as a [[lookup]] table in ROM instead of an array filled
at power-on. For sprites: fewer keyed [[sprites.group|sprite groups]] (each keeps 128 keys) or fewer slots in groups
sorted by depth. Each keyed sprite group's key tables take about 3 KiB of it.

```luau
-- Was: a 4096-entry heights array filled in a loop at power-on.
local height = lookup({x = {0, 255}}, function(x)
  return math.floor(math.sin(x * math.pi / 128) * 24 + 120)
end)

function frame(t, f)
  bg[1].scroll.y = height(f % 256) - 120
end
```
