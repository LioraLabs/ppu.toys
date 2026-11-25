---
id: hud-is-retained
kind: recipe
title: The HUD is retained: labels are written once in init, and fields and lines set every frame cost a compare
codes: []
fixture: title
---
[[@ppu/hud]] writes text into a background map and keeps it there. In `init`, write each fixed label once
with [[hud.text]] and make each changing value once: [[hud.field]] for a number, [[hud.line]] for text.
Then `frame` sets every field and line every frame without guarding it: a set compares with what is
shown and rewrites only the cells that changed, so an unchanged score costs one compare. Never call
`hud.text` every frame, and never use the deprecated `hud_text`/`hud_number`, which a cartridge refuses.
Labels and values sit side by side: the label's cells are not the field's. The map comes from a
[[hud-descriptor-names-placed-map|HUD descriptor]].

```luau
local hud = require("@ppu/hud")
local font = dma("font", {palette = "font_pal", char = 0x5000, pal = 112})
local HUD = {map = 0x7c00, width = 32, height = 28, palette = 7}
local MID = {map = 0x7c00, width = 32, height = 28, palette = 7, align = "center"}
local score = makeBounded(0, 0, 999)
local field, prompt

function init()
  mode = 1
  bg[1].char_base, bg[1].map_base = font.char, HUD.map
  screen.main.bg1 = true
  hud.text(HUD, 2, 1, "SCORE")
  field = hud.field(HUD, 8, 1, 3)
  prompt = hud.line(MID, 0, 20, 32)
end

function frame(t, f)
  if pad.pressed.a then score = math.min(999, score + 1) end
  field:set(score)
  prompt:set(if f // 30 % 2 == 0 then "PRESS START" else "")
end
```
