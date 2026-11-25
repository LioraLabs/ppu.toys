---
id: ppu-json-manifest
kind: concept
title: ppu.json lists a toy's code files and sources, and only what it lists is part of the toy
codes: []
---
`ppu.json` is the manifest: `title`, `description`, `files` (code files, in the order they run in a shared
global scope; a file another file `require`s runs as a module instead) and `sources`. `main.luau` (or
`main.lua`) must be listed. File names are `/`-separated paths that stay inside the folder: no `..`, no
absolute path, no leading dot, nothing a Windows disk refuses, and none that differ only in case. A source
has a `name`, a `kind` (`bg` tiled background, `sheet` ordered tiles, `obj` sprite cells, `m7` Mode 7
plane, or `palette`, a `.gpl`), a `file` (a PNG, or the `.gpl`; relative) and `options` (`bit_depth`,
`tile_size`, `cell_size`, `cell_height`, `extbg`, `dither`, `dither_strength`, `alpha_threshold`,
`max_palettes`, and `palette` to name a palette source; the importer supplies the defaults). A Mode 7
EXTBG source adds `priority_file`, a same-sized black-and-white PNG. A source has a `file` or a `payload`,
never both. See [[graphics-come-from-sources]] and [[connected-folder]].

```sh
cat ppu.json
```
