---
id: bring-art-in
kind: recipe
title: Art enters a project as an indexed PNG and its .gpl listed in ppu.json, from a folder or the Studio
codes: []
---
In a folder `ppu connect` links, each image source is `assets/<name>.png` (indexed) and each palette
`palettes/<name>.gpl`, all listed in `ppu.json`; saving one sends it to the project. `ppu art new`
makes empty art of a kind and size and lists it with its palette; `ppu art convert` turns a picture
drawn in any colours into SNES art ([[ppu-art-convert]]). Before placing it, `ppu art check` says
what an import would refuse ([[ppu-art-check]]).

An agent in the Studio adds art with `add_image_source`: a `grid` (one string per pixel row, `.`
transparent, every other character a key of `palette`) suits hand-drawn art, since long base64 is
easy to corrupt; an indexed PNG keeps its indices. `add_palette` adds a `.gpl` and `read_source`
returns a source as an indexed PNG to repaint. Then [[place-art-with-dma|place it with dma()]].

```sh
ppu art new assets/hero.png --kind obj --size 64x32 --cell 16 --new-palette hero_pal
ppu art check assets/hero.png
```
