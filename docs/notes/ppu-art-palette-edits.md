---
id: ppu-art-palette-edits
kind: recipe
title: ppu art palette sync, write and set change a palette without rewriting a PNG or ppu.json by hand
codes: []
fixture: twinstar
---
`ppu art palette sync palettes/ships_pal.gpl` rewrites the PLTE and transparency of every PNG whose source names
that palette, leaving pixel data byte-identical; it rewrites all in memory first, so a refusal (a pixel
past the palette) changes none. `ppu art palette write <gpl>` reads one `r g b` line (8-bit) per colour line
from stdin, snaps each to RGB555, keeps the file's header, names and comments, then syncs the PNGs and
prints `rewrote <png>` for each; a wrong count is refused. `ppu art palette set <png> <gpl> --width W
--height H` switches the art to another palette: index bytes in on stdin and out on stdout, each tile or
cell moved to the lowest row of the new palette holding all its colours exactly, the PNG and the source's
`options.palette` updated together. If a colour has no exact match nothing is guessed: exit 1, stderr lists
each missing `#rrggbb` with the tiles using it, no file changes.

```sh
ppu art palette sync palettes/ships_pal.gpl
{ printf '0 0 0\n255 0 0\n'; yes '0 0 0' | head -n 30; } | ppu art palette write palettes/ships_pal.gpl
```
