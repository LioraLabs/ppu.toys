---
id: remake-a-hud-field-after-overwriting-its-cells
kind: gotcha
title: Making a HUD field draws its cells, so make it again after something else overwrote them
codes: []
fixture: title
---
A [[hud.field]] draws its cells when it is made (a [[hud.line]] on its first `set`), and a later `set` rewrites only what changed. A
[[load-a-stage|stage load]], a `dma` into the same map, or a scene that reuses the map leaves the HUD's cells holding something else while
the field still believes it shows its last value, so `set` of the same number writes nothing. Make the field again in the code that
restores the map (a scene's `init` runs after every switch, which cold-boots, so fields made there are made again). Labels from [[hud.text]] need
writing again too. See [[hud-is-retained]].

```luau
local hud = require("@ppu/hud")
local font = dma("font", {palette = "font_pal", char = 0x5000, pal = 112})
local HUD = {map = 0x7c00, width = 32, height = 28, palette = 7}
local score = makeBounded(0, 0, 999)
local field

local function draw_hud()
  hud.text(HUD, 2, 1, "SCORE")
  field = hud.field(HUD, 8, 1, 3) -- made again: a new field draws its cells
end

function init()
  mode = 1
  bg[1].char_base, bg[1].map_base = font.char, HUD.map
  screen.main.bg1 = true
  draw_hud()
end

function frame(t, f)
  if pad.pressed.a then score = math.min(999, score + 1) end
  field:set(score)
end
```
