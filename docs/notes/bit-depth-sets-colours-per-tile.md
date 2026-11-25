---
id: bit-depth-sets-colours-per-tile
kind: concept
title: A source's bit depth is how many colours one tile can use, 4, 16 or 256, entry 0 of each being transparent
codes: []
fixture: twinstar
---
`bit_depth` in a source's options sets how many palette entries each tile addresses: 2bpp tiles use
a row of 4 colours (up to 8 rows, for Mode 0 and simple layers), 4bpp a row of 16 (backgrounds and
every sprite), 8bpp one block of 256 (rich backgrounds and Mode 7, which is always 8bpp). The first
entry of every row is transparent, so a row gives 3, 15 or 255 visible colours. Colours are the
SNES's 15-bit BGR555, 5 bits a channel; the importer reduces an image with too many and reports it.

A tile picks one row. In a background, a placement's `pal` is the CGRAM entry of the first row and a
tilemap cell's `pal` picks a row from there. Pick the smallest depth the art needs: a 2bpp tile is
half the VRAM of a 4bpp one.

```luau
-- 4bpp font at CGRAM 112 (row 7), 2bpp stars at entry 28 (row 7 of the 2bpp rows).
local font = dma("font", {palette = "font_pal", char = 0x5000, pal = 112})
local far = dma("far", {palette = "far_pal", char = 0x6000, map = 0x7800, pal = 28})
```
