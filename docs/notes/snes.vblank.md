---
id: snes.vblank
kind: diagnostic
title: Video memory written each frame must fall in at most two bounded regions the cartridge can upload in vblank
codes: [snes.vblank]
---
A cartridge copies the video memory a frame changes during vertical blank, the short gap between
frames, and plans those copies when it builds: at most two regions, 1,536 words in all, with OAM
and palettes sharing the time. `error[snes.vblank]` means it could not plan them: a frame writes
cells at addresses it cannot bound, or the maps written each frame are too far apart or too large.

Keep every per-frame write inside the map it updates: clamp or wrap the cell's column and row.
Put art and maps that never change in `init()` or a file-level [[dma]], not in `frame`. Keep the
maps updated each frame close together. A level wider than the screen is not drawn cell by cell
from `frame`: make it a [[tiled-maps-for-big-levels|Tiled map]] loaded with [[@ppu/world]],
whose follow uploads only the rows and columns the camera uncovers.

```luau
local cursor = makeBounded(0, 0, 31)

function init()
  bg[1].map_base = 0x7c00
  bg[1].screen_size = 0
  for x = 0, 31 do bg[1].map[x] = {} end
end

function frame(t, f)
  if pad.pressed.right then cursor = math.min(cursor + 1, 31) end
  if pad.pressed.left then cursor = math.max(cursor - 1, 0) end
  bg[1].map[cursor][20] = {tile = 1}
end
```
