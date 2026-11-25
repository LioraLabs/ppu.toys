---
id: hud-text-is-printable-ascii
kind: rule
title: HUD text is printable ASCII and no wider than its line; anything else is an error
codes: []
fixture: title
---
[[hud.text]] and a [[hud.line]]'s `set` draw characters from space to `~` only. A newline, a tab, an accented
letter or an emoji is an error, and so is text longer than the line's cells. Write each row of a message as
its own line, and keep each string within the width you gave the line.

```luau
local hud = require("@ppu/hud")
local font = dma("font", {palette = "font_pal", char = 0x5000, pal = 112})
local HUD = {map = 0x7c00, width = 32, height = 28, palette = 7}
local top, bottom

function init()
  mode = 1
  bg[1].char_base, bg[1].map_base = font.char, HUD.map
  screen.main.bg1 = true
  top = hud.line(HUD, 2, 10, 28)
  bottom = hud.line(HUD, 2, 11, 28)
end

function frame(t, f)
  top:set("THE GATE IS OPEN.") -- two lines, not one string with "\n"
  bottom:set("FIND THE KEY!")
end
```
