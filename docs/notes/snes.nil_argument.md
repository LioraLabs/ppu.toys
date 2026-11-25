---
id: snes.nil_argument
kind: diagnostic
title: A helper called with an argument left out gets nil, which a cartridge cannot hold in a number
codes: [snes.nil_argument]
---
A cartridge has no nil: a number parameter is always a number, and a boolean parameter holds
`false` where Lua would hold nil. So a call that leaves an argument out (or passes nil) is refused
when the helper uses that parameter in a way where nil matters: arithmetic, a comparison with nil,
or passing it on. The message names the parameter and the line that uses it.

Pass every argument explicitly: a number, or `true`/`false` for a flag. For a number parameter you
may instead default it at each use, `by or 1`. For a flag, test its truth (`if fast then`) rather
than comparing it with nil.

```luau
local function step(x, by)
  return x + by
end

local pos = makeBounded(0, 0, 255)

function frame(t, f)
  pos = step(pos, 1) % 256 -- was step(pos): `by` was nil
  bg[1].scroll.x = pos
end
```
