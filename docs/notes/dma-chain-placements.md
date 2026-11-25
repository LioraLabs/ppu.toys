---
id: dma-chain-placements
kind: recipe
title: Sources that share VRAM chain from the next_char, next_map and next_pal that dma returns
codes: []
fixture: twinstar
---
[[dma]] returns where a source went and the next free aligned address after it: `next_char` and
`next_map` (backgrounds), `next_char` and `next_pal` (sheets and sprites), `next_pal` (palettes),
`next_addr` (samples). Start the next placement there instead of working out sizes by hand. A sprite
result also carries `tile`, its first tile counted from the sprites' base, and `cells` for
[[sprites.art]]; a background's carries `bit_depth` and `screen_size`.

Backgrounds drawn in the same mode often share one palette base on purpose: give them the same
`pal` rather than `next_pal`. Later placements win where colours overlap, as hardware writes do.
A sprite sheet with cells wider than 8 pixels must start on a 16-tile row ([[dma-alignment]]), so
round its `next_char` up to one before chaining another sheet after it.

```luau
-- The font's tiles end inside 0x5000..0x5fff, so the stars start at its next_char, 0x6000.
local font = dma("font", {palette = "font_pal", char = 0x5000, pal = 112})
local far = dma("far", {palette = "far_pal", char = font.next_char, map = 0x7800, pal = 28})
```
