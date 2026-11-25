---
id: snes.surface
kind: diagnostic
title: A cartridge joins modules at build time, so a module is used through its fields
codes: [snes.surface]
fixture: title
---
A cartridge joins a toy's files into one program when it builds, so `require` must name something
it can find then and a module must be used in ways it can resolve. `error[snes.surface]` names one
it could not: a library name that is not one of the `@ppu/` libraries; two files whose names differ
only in `/`, `-` or `_`; a module used as a value (passed around or picked at run time), indexed by
anything but a field name (`State[k]`), or read for a field its returned table does not have.

Use a module through its fields, `Shared.HUD`; a local may name it once (`local s = Shared`). Fix a
library's name, or add the module and require it as `"./name"`.

```luau
local hud = require("@ppu/hud")
local font = dma("font", {palette = "font_pal", char = 0x5000, pal = 112})
local HUD = {map = 0x7c00, width = 32, height = 28, palette = 7}

function init()
  mode = 1
  bg[1].char_base, bg[1].map_base = font.char, HUD.map
  screen.main.bg1 = true
  hud.text(HUD, 11, 13, "SCORE")
end
```
