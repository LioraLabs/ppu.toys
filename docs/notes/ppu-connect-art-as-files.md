---
id: ppu-connect-art-as-files
kind: concept
title: ppu connect brings art into the folder as indexed PNGs and .gpl palettes, and holds every save to the art rules
codes: []
---
A pulled project's image sources (`bg`, `sheet`, `obj`, `m7`) become indexed PNGs, `assets/<name>.png`, and
its palettes `palettes/<name>.gpl`, all listed in `ppu.json`; a PNG that a Tiled tileset in the folder names
lands at that path so Tiled finds it. Art that names no palette gets one made from its colours and `connect`
says so once. A source that cannot be a file (an unsafe name, a PNG that would not import back to the same
payload) stays a `.ppu/sources/N.bin` payload, as do samples and songs. Each save in the folder goes to the
project once it settles, after the checks of [[ppu-art-check]]; art the site changed is rendered over the PNG
and the old one moves to `.ppu-backup/<revision>/`, and a changed palette makes every PNG naming it follow
([[ppu-art-palette-edits]]). Pull one with `ppu connect <project link> my-toy`. A Studio tab on the project shows "folder connected".

```sh
ls assets palettes
```
