---
id: ppu-pack-and-unpack
kind: recipe
title: ppu pack folds a toy folder into one .ppu.json file, and ppu unpack turns it back into a folder
codes: []
---
`ppu pack <dir> -o toy.ppu.json` writes one file holding every code file in `ppu.json`'s `files` (in that
order) and every source's converted graphics, so it needs no asset files beside it and opens in the
Studio. It checks the manifest and converts PNGs but does not run the code, so [[ppu-check|check]] the
packed file too (`ppu check toy.ppu.json`); it must list `main.luau` or `main.lua`, and refuses an unsafe
or case-twin file name. `ppu unpack <file> <dir>` writes a folder back and overwrites files with the same
names, so give it a fresh directory. Every source comes back as a payload under `.ppu/sources/`, not as a PNG,
because the PNG cannot be recovered from the converted data: keep the editable folder.
Publishing stays a Studio action.

```sh
ppu pack . -o toy.ppu.json
ppu unpack toy.ppu.json editable-copy
```
