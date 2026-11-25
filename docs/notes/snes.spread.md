---
id: snes.spread
kind: diagnostic
title: Work spread over several updates is a routine that yields, not a spread annotation
codes: [snes.spread]
---
`error[snes.spread]` means the source still carries `---@snes spread` (or calls `spread(...)`),
which no longer exists. Write the work as a [[routine]]: its body runs a little on each `step()` and
pauses at [[yield|yield()]], keeping its locals and loop positions. Start it once and step it once
an update. Without a routine, keep a cursor yourself and handle a few items per update.

```luau
local tiles = {}
for i = 1, 64 do tiles[i] = 0 end

local fill = routine(function()
  for i = 1, 64 do
    tiles[i] = i % 8
    if i % 16 == 0 then yield() end
  end
end)

function init()
  fill:start()
end

function frame(t, f)
  fill:step()
end
```
