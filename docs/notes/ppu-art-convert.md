---
id: ppu-art-convert
kind: recipe
title: ppu art convert turns a full-colour PNG into indexed SNES art and stores its options in ppu.json
codes: []
---
`ppu art convert <png>` runs the same converter the site runs, offline: it rewrites the PNG as indexed
art, writes its palette to `palettes/<name>_pal.gpl` (`--new-palette` to name it), lists both in
`ppu.json`, and copies the original to `.ppu-backup/` first. A PNG the manifest lists converts with its
entry's kind and options; one it does not list needs `--kind` (`bg`, `sheet`, `obj`, `m7`).

The flags are the source's options, stored so a later import makes the same art: `--bpp` (2, 4 or 8
for `bg` and `sheet`), `--cell` and `--cell-height` (`obj`), `--dither` (`none`, `bayer`,
`diffusion`) and `--dither-strength`, `--alpha-threshold` (alpha at or above is opaque, default 128),
`--max-palettes` (1 to 8). Start without dither: diffusion smooths gradients but makes many distinct
tiles. A flag the kind does not read is refused, and a refused picture changes no file.

```sh
mkdir -p assets
echo iVBORw0KGgoAAAANSUhEUgAAAAgAAAAICAYAAADED76LAAAAEklEQVR4nGM4YWPzHx9mGBkKAA++j8FWgB/4AAAAAElFTkSuQmCC | base64 -d > assets/photo.png
ppu art convert assets/photo.png --kind obj --cell 8 --alpha-threshold 160
ppu art check assets/photo.png
```
