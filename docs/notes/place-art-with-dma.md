---
id: place-art-with-dma
kind: recipe
title: Art is an image source named in ppu.json and placed once at file level with dma()
codes: []
fixture: twinstar
---
Graphics come from image sources listed in the project's ppu.json (PNG files with a kind: `bg`,
`sheet`, `obj`, `m7`, and `palette` sources for their colours). [[dma|dma(name, placement)]] copies a
source's tiles to VRAM and its colours to CGRAM and returns where they went: `char`, `map`, `pal` and,
for a sheet, the `cells` that [[sprites.art]] cuts into sprites.

Call it at file level, once, not inside `frame`: a placement is a load, and on a cartridge a file-level
`dma` runs at power-on. Give `char`, `map` and `pal` yourself when several sources share VRAM, so they
do not overlap. Never fill VRAM with `vram[]` loops in `init()`: a console shows black for seconds.

```luau
local font = dma("font", {palette = "font_pal", char = 0x5000, pal = 112})
local ships = dma("ships", {palette = "ships_pal", char = 0x4000, pal = 0})
local ship = sprites.art({{size = 16, columns = 1, cells = {0}}}, ships)
local group = sprites.group()

function frame(t, f)
  group:draw(ship, 120, 104)
end
```
