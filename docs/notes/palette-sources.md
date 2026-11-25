---
id: palette-sources
kind: concept
title: A palette source is a .gpl of colour rows; art wears it only when dma names it with palette
codes: []
fixture: twinstar
---
A palette source is a GIMP `.gpl` file (`GIMP Palette` header, optional `Name:` and `Columns:`, one
`R G B [name]` line per colour, channels 0 to 255 reduced to 5 bits). `Columns:` is the row size: 4
for 2bpp, 16 for 4bpp and sprites, 256 for 8bpp and Mode 7; without it a row is 16. Colours fill
rows in order, a short last row is padded with its own first colour, and a palette holds at most 256.
Entry 0 of each row is transparent: kept in the file, never written to CGRAM.

`dma("art", {palette = "art_pal", ...})` writes the art's tiles in that palette's colours, row for
row; the row size must match the art's depth. Without `palette` the art keeps the colours it was
imported with, and editing the `.gpl` never reaches it. A palette also places alone: `pal` is the
CGRAM entry of its first row (0 to 255), and it takes no `char` or `map`. Two images share colours
by naming the same palette.

```luau
-- The ships in their own palette, then the same colours again from sprite palette 4 (CGRAM 192).
local ships = dma("ships", {palette = "ships_pal", char = 0x4000, pal = 0})
local spare = dma("ships_pal", {pal = 192})
```
