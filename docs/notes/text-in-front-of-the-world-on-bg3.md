---
id: text-in-front-of-the-world-on-bg3
kind: recipe
title: Text in front of the world and its sprites goes on bg3 with a 2bpp font, priority 1 and bg3_priority
codes: []
fixture: adventure
---
In Mode 1, bg1 and bg2 hold the world and bg3 is a 4-colour (2bpp) layer that normally draws behind them.
Setting [[bg3_priority]] lifts bg3's priority-1 tiles over everything, sprites included: the layer for a
dialog box over the map. Import the font at bit depth 2, place it with its palette at `pal = 28` (palette 7
of a 2bpp layer is CGRAM 28..31), and give the [[hud-descriptor-names-placed-map|HUD descriptor]]
`priority = 1` so every glyph it writes is a priority-1 tile. A HUD over a scrolling world that does not
need to cover sprites can stay on bg1 with a 4bpp font (`pal = 112`), as most templates do; a Mode 7 game
puts its text on bg2.

```luau
local hud = require("@ppu/hud")
local font = dma("font", {palette = "font_pal", char = 0x5000, pal = 28})
local HUD = {map = 0x5400, width = 32, height = 28, palette = 7, priority = 1}

function init()
  mode = 1
  bg[3].char_base, bg[3].map_base = font.char, HUD.map
  bg3_priority = true
  screen.main.bg3 = true
  hud.text(HUD, 3, 21, "Mind the moths.")
end

function frame(t, f)
end
```
