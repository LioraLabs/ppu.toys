---
id: vram-map-comment
kind: rule
title: Write the VRAM and CGRAM map as a comment at the top of shared.luau
codes: []
fixture: title
---
Every [[dma]] placement, text map and layer base names an address, and nothing warns when two overlap:
the screen just shows the wrong tiles. Keep one comment at the top of shared.luau that lists each VRAM
range (in words) and each CGRAM range, and change it with the code. It is also where a scene's own
placements look for free room (see [[placements-a-scene-owns]]).

```luau
-- VRAM, in words:  0x0000..0x3fff  free for the backgrounds
--                  0x4000..0x43ff  the sprites' cells
--                  0x5000..0x55ff  the font, 96 tiles
--                  0x7c00..0x7fff  the text map, 32x32 cells
-- CGRAM:  112..127 the font (palette 7), 128..143 the sprites (sprite palette 0).
local sheet = dma("sprites", {palette = "sprites_pal", char = 0x4000, pal = 0})
local font = dma("font", {palette = "font_pal", char = 0x5000, pal = 112})

function init()
  mode = 1
  bg[1].char_base, bg[1].map_base = font.char, 0x7c00
  obj.char_base = sheet.char
  screen.main.bg1, screen.main.obj = true, true
end
```
