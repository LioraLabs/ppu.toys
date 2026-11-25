---
id: ppu-art-check
kind: rule
title: ppu connect sends no art save that ppu art check refuses; the project keeps its last good version
codes: []
---
`ppu art check <png>` reports what importing the PNG as its `ppu.json` source would refuse: a tile
(a cell, for sprites) that mixes palette rows, a row past the palette budget, more distinct tiles than
the kind keeps, a PNG palette that no longer matches its `.gpl`, or a PNG that is not indexed SNES art
(full colour, or its transparency lost). Entry 0 of each row is transparent and never counts as a
colour of the row.

A pixel editor never blocks a save, so `ppu connect` holds each save to these rules instead: a save
that breaks one prints the reason and sends nothing. Fix the tile and save again. A palette edit that
cannot apply (a pixel past the palette's colours) is refused the same way.

```sh
ppu art check assets/sprites.png
```
