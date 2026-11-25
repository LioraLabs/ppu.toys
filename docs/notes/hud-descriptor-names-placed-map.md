---
id: hud-descriptor-names-placed-map
kind: concept
title: A HUD descriptor names a map and a font palette the program placed; the library allocates nothing
codes: []
fixture: title
---
`{map = 0x7c00, width = 32, height = 28, palette = 7}` is all [[@ppu/hud]] knows: the word address of a
32x28 tile map in VRAM, and which background palette its glyphs use. The program does the rest. It places
the font with [[dma|dma()]] (its `pal` is the palette's first colour: 112 is palette 7 of a 4bpp layer),
points a background's `char_base` at the font and `map_base` at the descriptor's map, and turns the layer on,
in the `setup()` every scene's init calls. Write the VRAM map in a comment so the font, the text map and the
art never overlap. Further fields shape the text: `padding = "0"` for number fields, `align = "center"`,
`glyph = "16x16"` with `font =` the first tile of a big font, and `priority = 1`
([[text-in-front-of-the-world-on-bg3|text in front]]). Several descriptors may share one map.

```luau
local hud = require("@ppu/hud")
-- VRAM, in words: font 0x5000, big font 0x6000 (tile 256 of BG1's tiles), text map 0x7c00.
local font = dma("font", {palette = "font_pal", char = 0x5000, pal = 112})
local _big = dma("font16", {palette = "font_pal", char = 0x6000, pal = 112})
local HUD = {map = 0x7c00, width = 32, height = 28, palette = 7}
local BIG = {map = 0x7c00, width = 32, height = 28, palette = 7, glyph = "16x16", font = 256, align = "center"}

function init()
  mode = 1
  bg[1].char_base, bg[1].map_base = font.char, HUD.map
  screen.main.bg1 = true
  hud.text(BIG, 0, 7, "TITLE SCREEN")
  hud.text(HUD, 11, 13, "SCORE")
end

function frame(t, f)
end
```
