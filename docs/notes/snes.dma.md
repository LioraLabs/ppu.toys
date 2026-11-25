---
id: snes.dma
kind: diagnostic
title: dma stops the program when the source is missing or the placement does not fit, align or stay clear
codes: [snes.dma]
fixture: twinstar
---
`error[snes.dma]: dma: ...` names what [[dma]] refused:

- `no source named 'x'`: the name is not in `ppu.json`'s `sources` (names are exact; a file name is not a source name).
- `opts.palette must be a palette source name`, or rows that do not fit: `palette` names a source
  that is not a palette, or one whose row size is not the art's (4, 16 or 256 colours).
- `is a palette source / an m7 source / a sample source, it takes only ...`: that kind takes no
  `char`/`map` (palette), no addresses at all (m7), or only `addr` (sample).
- `must align`, `exceeds PPU memory`, `obj chars must fit the shared 512-tile region`: see
  [[dma-alignment]].
- `overlaps VRAM already in use`, `conflicts with CGRAM already in use`: two setup placements claim
  the same words; move one, chaining from the first's `next_char` ([[dma-chain-placements]]).
- on a cartridge, `dma("name")` takes the source's name as a string literal and at most one constant table of
  whole numbers named `char`, `map`, `pal` and `palette` (the palette's name, also a literal); it places images and
  palettes only, so a sound is a `bank` and a song a `score`.
- `dma runs during setup, or in frame() while force_blank is true`: a load from `frame()` needs a
  blanked screen ([[load-a-stage]]); hooks and timers never place.

```luau
-- The sprites at 0x4000 and the font at 0x5000: two placements, no shared words.
local ships = dma("ships", {palette = "ships_pal", char = 0x4000, pal = 0})
local font = dma("font", {palette = "font_pal", char = 0x5000, pal = 112})
```
