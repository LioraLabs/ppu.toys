---
id: source-budgets
kind: rule
title: Each image source kind has a size and tile limit, and VRAM placement is tighter than either
codes: []
fixture: twinstar
---
Tile art is 8x8 cells, so a PNG's sides are multiples of 8 (of the cell, for `obj`). Each kind keeps
its own limit, and the importer reports what it cut instead of hiding it:

- `bg`: up to 512x512 pixels (64x64 map cells), at most 1024 distinct tiles, the transparent one included.
- `sheet`: 8x8 cells in row order, at most 1024 tiles, kept as drawn (no tile is merged).
- `obj`: cells 8 to 128 pixels on a side in steps of 8 (`cell_size`, `cell_height`); all sprite
  sheets share one 512-tile region of VRAM.
- `m7`: a plane up to 1024x1024 pixels with at most 256 distinct tiles; a repeated tile costs nothing.

Distinct tiles cost VRAM, not pixels: a big repeating image can cost fewer tiles than a small
detailed one. A 2bpp tile takes 8 words, 4bpp 16 and 8bpp 32, out of 32K words that every layer's
tiles and maps share; [[dma]] refuses a placement that does not fit ([[snes.dma]]).

```luau
-- The far stars are 2bpp: 8 words a tile, at 0x6000, map at 0x7800.
local far = dma("far", {palette = "far_pal", char = 0x6000, map = 0x7800, pal = 28})
local after = far.next_char -- the first free aligned address after its tiles
```
