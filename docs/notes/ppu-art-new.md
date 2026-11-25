---
id: ppu-art-new
kind: recipe
title: ppu art new makes an empty indexed PNG and lists it as a source, so a pixel editor has valid art to start from
codes: []
---
`ppu art new assets/hero.png --kind obj --size 64x32 --cell 16 --new-palette hero_pal` writes a PNG of index 0, the
palette, and the source in the nearest `ppu.json` (the project is the nearest `ppu.json` above the PNG),
printing `wrote <path>` for each. `--kind` is `bg`, `sheet`, `obj` or `m7`; `--size` is a multiple of the
tile (an `obj` of its `--cell`, 8 to 128), at most 1024 a side. `--bpp` is 2, 4 or 8 for `bg` and `sheet`
(default 4); `obj` is 4 and `m7` 8. Give exactly one of `--palette file.gpl` (an existing palette whose
rows match the art's 4, 16 or 256) and `--new-palette name` (writes `palettes/<name>.gpl`: entry 0 and a
grey ramp). The name differs from the art's, because sources share one namespace (`hero_pal` for
`hero.png`). It exits 2 and writes nothing for a PNG that exists, a source with that name, a size off the
tile grid or a `bg` over 512 px; its PNG passes [[ppu-art-check]] straight away.

```sh
ppu art new assets/hero.png --kind obj --size 64x32 --cell 16 --new-palette hero_pal
ppu art check assets/hero.png
```
