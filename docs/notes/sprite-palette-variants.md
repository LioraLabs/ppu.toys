---
id: sprite-palette-variants
kind: recipe
title: Colour variants of one sheet come from a level's palette and pal, with no new tiles
codes: []
fixture: adventure
---
Villagers in three colourings, or a red and a blue team, are the same tiles in other colours. Draw each
recolouring as a palette source (a `.gpl` the sheet's palette recoloured, listed in ppu.json), and name it in
the art's level: `palette = "npc1"` with `pal = 4` loads those colours into OBJ palette 4 and draws the
cells with it. The tiles stay where [[dma|dma()]] put them once, so a variant costs 16 colours of CGRAM and
no VRAM. Give each variant its own OBJ palette number. For one draw in another of the palettes already
loaded, the draw's `pal` option is enough.

```luau
local sheet = dma("hero", {palette = "hero_pal", char = 0x6000, pal = 0})
local HERO = sprites.art({{size = 32, columns = 1, cells = {0}}}, sheet)
local BAKER = sprites.art({{size = 32, columns = 1, cells = {0}, palette = "npc1", pal = 4}}, sheet)
local SMITH = sprites.art({{size = 32, columns = 1, cells = {0}, palette = "npc2", pal = 5}}, sheet)
local folk = sprites.group()

function init()
  mode = 1
  screen.main.obj = true
  obj.char_base = sheet.char
end

function frame(t, f)
  folk:draw(HERO, 80, 100)
  folk:draw(BAKER, 128, 100)
  folk:draw(SMITH, 176, 100)
end
```
