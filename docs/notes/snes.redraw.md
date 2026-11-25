---
id: snes.redraw
kind: diagnostic
title: A loop that draws cells skips the unchanged ones only when nothing else writes those cells every update
codes: [snes.redraw]
---
A cartridge reruns a loop that writes tilemap cells or sprites only for the entries that changed
since the last update, which keeps redraws cheap. The warning `... writes these cells on every
update, so this loop cannot skip the ones that did not change` names another place that writes the
same video memory each update. With two writers, the loop cannot know which cells still hold its
values, so it redraws all of them every time.

Draw these cells from one loop, give each loop its own range of cells, or write them from the other
place only when they change.

```luau
local row = {}
for x = 1, 32 do row[x] = 1 end

function init()
  bg[1].map_base = 0x7c00
  bg[1].screen_size = 0
  for x = 0, 31 do bg[1].map[x] = {} end
end

function frame(t, f)
  row[f % 32 + 1] = f // 32 % 4 + 1
  for x = 1, 32 do
    bg[1].map[x - 1][20] = {tile = row[x]}
  end
end
```
