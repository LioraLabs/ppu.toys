---
id: snes.rom
kind: diagnostic
title: A cartridge's ROM holds the code, the art and every baked table, so big grids and unused data overrun it
codes: [snes.rom]
---
`error[snes.rom]` means the program's code and data do not fit the cartridge's ROM size: the art and
sounds, every [[lookup]] and [[bakeHdma]] table (one entry per grid point), and the generated code.
The message says how many bytes are over. `warning[snes.rom]` is the check's estimate saying the
cartridge is nearly full: the build still fits it, but the next table or map may not.

Shrink what is baked first: give each grid input its honest range and a coarser step, since a table
grows with every point of its grid. Remove art and data the game no longer uses. If the game needs
the room, choose a larger ROM size in the Cartridge settings (`romSize` in ppu.json).

One piece bigger than a 32 KiB ROM bank is refused whatever the ROM size: each run of VRAM one
[[dma]] call writes is stored whole in one bank, and so are the tile ids (2 bytes a cell) of a layer
`w:set` changes and a Mode 7 plane's table. Split the art into smaller sources placed by separate
`dma()` calls, or make the map smaller or drop its `w:set`. A single function or table over a bank is refused the same way (the message names the section): split the function, or shrink the table.

```luau
-- 64 entries, not 1024: the wave needs a step of 1 over 0..63.
local wave = lookup({phase = {0, 63}}, function(phase)
  return math.floor(math.sin(phase * math.pi / 32) * 16)
end)

function frame(t, f)
  bg[1].scroll.y = wave(f % 64)
end
```
