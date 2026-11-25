---
id: dma-alignment
kind: rule
title: dma addresses are VRAM word addresses, and each kind of source aligns its tiles and map
codes: []
fixture: twinstar
---
VRAM holds 32K words, `0x0000` to `0x7fff`, and every `char` and `map` is a word address. A `bg`
places its tiles on a multiple of `0x1000` and its map on a multiple of `0x400`; a `sheet` its tiles
on `0x1000`. The first sprite sheet aligns to `0x2000` and fixes the sprites' 8K-word region
(`obj.char_base`): every other sheet goes inside it, chained by tile. A sheet with cells wider than 8
pixels fills whole 16-tile rows, so start the next one on a row (a multiple of `0x100` words from the
base). An `m7` source takes no addresses: the plane always sits at `0x0000` to `0x3fff` with its
colours from CGRAM 1.

Give the addresses yourself when several sources share VRAM, and keep a plan of them in a comment, as
the templates do. A placement that breaks a rule stops the program with [[snes.dma]].

```luau
-- Sprites at 0x4000 (the region 0x4000..0x5fff), the font at 0x5000, stars at 0x6000, maps from 0x7400.
local ships = dma("ships", {palette = "ships_pal", char = 0x4000, pal = 0})
local bits = dma("bits", {palette = "bits_pal", char = 0x4800, pal = 6})
local font = dma("font", {palette = "font_pal", char = 0x5000, pal = 112})
local far = dma("far", {palette = "far_pal", char = 0x6000, map = 0x7800, pal = 28})
```
