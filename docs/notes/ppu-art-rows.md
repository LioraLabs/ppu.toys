---
id: ppu-art-rows
kind: recipe
title: ppu art rows moves tiles between palette rows, or fixes every tile that mixes rows
codes: []
fixture: twinstar
---
A tile (or sprite cell) takes its colours from one palette row, so a tile that mixes rows is refused on
import ([[ppu-art-check]]). `ppu art rows <png> --width W --height H` reads the index bytes on stdin and
prints the changed bytes on stdout with a summary on stderr. `--fix` gives each tile the row most of its
non-transparent pixels use (ties go to the lowest row) and moves a pixel of another row to the first entry
of its row with exactly its colour; a pixel with no such entry stays, and stderr says `tile x,y: n pixels
have no colour in row r`. `--set ROW --rect X,Y,W,H` moves every tile the rectangle touches to `ROW`
(`p -> ROW * row + p % row`); a row past the palette or a rectangle outside the image is refused. 8 bpp and
Mode 7 art is one 256-colour block with no rows, and is refused.

```sh
head -c 3328 /dev/zero > indices.bin
ppu art rows assets/ships.png --width 208 --height 16 --fix < indices.bin > fixed.bin
ppu art rows assets/ships.png --width 208 --height 16 --set 1 --rect 8,0,16,8 < indices.bin > moved.bin
```
