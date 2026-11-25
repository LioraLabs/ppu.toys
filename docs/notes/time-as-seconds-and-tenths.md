---
id: time-as-seconds-and-tenths
kind: recipe
title: A time shows as seconds and tenths in two HUD fields either side of a dot written in the label
codes: []
fixture: racer
---
Count the time in frames, a bounded whole number. A [[hud.field]] shows whole numbers only, so a lap time
is two fields: seconds (`frames // 60`) and tenths (`frames % 60 // 6`, at 60 frames a second). The label
written once with [[hud.text]] carries the dot between them (`"TIME    ."`), and the two fields sit
either side of it, each set every frame ([[hud-is-retained|retained HUD]]). Clamp the counter to its bound
so the fields never overflow their width.

```luau
local hud = require("@ppu/hud")
local font = dma("font", {palette = "font_pal", char = 0x5000, pal = 112})
local HUD = {map = 0x7c00, width = 32, height = 28, palette = 7}
local time = makeBounded(0, 0, 35999)
local seconds, tenths

function init()
  mode = 1
  bg[1].char_base, bg[1].map_base = font.char, HUD.map
  screen.main.bg1 = true
  hud.text(HUD, 18, 2, "TIME    .")
  seconds, tenths = hud.field(HUD, 23, 2, 3), hud.field(HUD, 27, 2, 1)
end

function frame(t, f)
  time = math.min(35999, time + 1)
  seconds:set(time // 60)
  tenths:set(time % 60 // 6)
end
```
