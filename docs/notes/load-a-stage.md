---
id: load-a-stage
kind: recipe
title: New graphics mid-game load from frame() under force_blank, and show from the next unblanked update
codes: []
fixture: twinstar
---
A placement at file level loads once, at power-on (in a program of scenes, each scene's own file-level
`dma` loads when the scene starts). To swap graphics inside a scene, the way a game changes stage:
set [[force_blank]], call [[dma]] from `frame()` (or a helper it calls), and keep it blanked until
`frame()` returns; unblank on a later update. A cartridge copies the data while the screen is dark,
about 21K words a video frame, so a 16K-word Mode 7 ground takes most of one.

A load writes over whatever its addresses held, overlap allowed: that is its purpose. It is checked
for alignment and bounds as at setup. Samples load only at setup. If code reads the load's result,
track the load in a flag so the handle is read only after it is assigned.

```luau
local loaded = false

function frame(t, f)
  if pad.a and not loaded then
    force_blank = true
    dma("near", {palette = "near_pal", char = 0x0000, map = 0x7400, pal = 32})
    loaded = true
  else
    force_blank = false
  end
end
```
