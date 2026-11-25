---
id: snes.bank7f
kind: diagnostic
title: Work RAM bank $7F holds the scanline tables, world buffers and run-time state, and a bigger cartridge does not add to it
codes: [snes.bank7f]
---
`error[snes.bank7f]` means the toy claims more of work RAM bank $7F's 64 KiB than it has. The message lists the
claims: scanline tables (each full-screen [[hdma|hdma()]] or [[bakeHdma]] stream holds 898 to 2,242 bytes), the baked
`raster.mode` table, the map cells `w:set` can change, tile animation frames, and run-time state. It is work RAM, so
cartridge size does not help.

Cut what the message names first: fewer registers written per row in full-screen schedules, a schedule over only the
rows that change instead of 0..223, smaller maps for worlds that `w:set`, fewer animated tiles.

```luau
-- Was: hdma(0, 223, ...) writing both scroll.x and color.fixed on every row.
-- Only the sky band waves, and the colour is set once for the frame.
local wave = bakeHdma({phase = {0, 31}}, 0, 63, function(y, phase)
  bg[1].scroll.x = math.floor(math.sin((y + phase) * math.pi / 16) * 3)
end)

function frame(t, f)
  wave(f % 32)
end
```
