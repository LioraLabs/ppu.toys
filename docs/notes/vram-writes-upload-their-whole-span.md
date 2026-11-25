---
id: vram-writes-upload-their-whole-span
kind: gotcha
title: A frame's writes to video memory upload everything from the lowest to the highest word written, so two far-apart cells cost the span between them
codes: [snes.vblank]
fixture: title
---
On a cartridge, the map cells a frame writes are copied in the next vertical blank as one block, from the
lowest address written to the highest. Two cells a thousand words apart cost the same as redrawing the
thousand words between them, about 5% of a frame, and use the blank's room ([[snes.vblank]]). Keep cells a
frame changes near each other: one HUD row, one block of a map. A HUD at the top and a counter at the bottom
of the same map belong in one band of rows.

```luau
local font = dma("font", {palette = "font_pal", char = 0x5000, pal = 112})
local tick = makeBounded(0, 0, 9)

function init()
  mode = 1
  bg[1].char_base, bg[1].map_base = font.char, 0x7c00
  screen.main.bg1 = true
end

function frame(t, f)
  tick = f // 6 % 10
  vram[0x7c00 + 32 + 1] = 16 + tick -- both writes in row 1: a span of three words
  vram[0x7c00 + 32 + 3] = 16 + tick
end
```
