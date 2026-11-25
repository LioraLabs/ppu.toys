---
id: hud-field-holds-whole-numbers-below-its-width
kind: rule
title: A HUD field shows whole numbers from 0 up to all nines of its width, and stops on anything else
codes: []
fixture: title
---
A [[hud.field]] made `width` cells wide (1 to 9) shows a whole number from 0 to 10^width - 1: 0 to 999 for
three cells. A negative number, a fraction or one with too many digits is an error, which stops a cartridge.
Bound the value with [[makeBounded]] to the field's range and clamp it where it grows (`math.min(999, score +
n)`); show a sign or a unit as text beside the field.

```luau
local hud = require("@ppu/hud")
local font = dma("font", {palette = "font_pal", char = 0x5000, pal = 112})
local HUD = {map = 0x7c00, width = 32, height = 28, palette = 7}
local score = makeBounded(0, 0, 999)
local field

function init()
  mode = 1
  bg[1].char_base, bg[1].map_base = font.char, HUD.map
  screen.main.bg1 = true
  field = hud.field(HUD, 2, 1, 3)
end

function frame(t, f)
  if pad.pressed.a then score = math.min(999, score + 25) end
  field:set(score)
end
```
