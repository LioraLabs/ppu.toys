---
id: sram-is-preview-only-use-savedata
kind: gotcha
title: The sram table is preview-only, so a game that saves writes a saveData record instead
codes: []
---
`sram` is a free-form table the preview keeps between reloads, one per toy in the player's browser. A
cartridge cannot hold it and refuses the program: its battery RAM has a fixed layout the build must
see. Declare the save as one [[saveData]] record of bounded fields, at file level, and read and assign
its fields as you would any table's ([[snes.save]], [[save-data-one-flat-record]]). The same record
works in the preview, where it also survives a reload, and on a cartridge.

```luau
local save = saveData({best = makeBounded(0, 0, 9999)})
local score = makeBounded(0, 0, 9999)

function frame(t, f)
  if pad.pressed.a then score = math.min(9999, score + 10) end
  if score > save.best then save.best = score end
end
```
