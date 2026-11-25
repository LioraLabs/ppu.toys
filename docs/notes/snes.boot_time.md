---
id: snes.boot_time
kind: diagnostic
title: Power-on code that loops over vram[] keeps a console's screen black for seconds; place art with dma() instead
codes: [snes.boot_time]
fixture: twinstar
---
The warning `Power-on (boot and init()) takes about N seconds` is the cartridge's estimate of the work before the
first picture: file-level code and `init()`. The screen stays black until it finishes. The message names the lines that
cost most, almost always loops writing `vram[]`, `cgram[]` or `bg[n].map` tile by tile.

Draw the graphics as an image source and [[place-art-with-dma|place it with dma()]]: the cartridge keeps it in ROM and
copies it in one DMA at power-on. Work that truly must run at start but is long (generating a level) goes in a
[[routine]] stepped a little each update behind a title screen.

```luau
-- Was: for i = 0, 0x3FFF do vram[i] = ... end in init().
local far = dma("far", {palette = "far_pal", char = 0x6000, map = 0x7800, pal = 28})

function frame(t, f)
  bg[1].char_base, bg[1].map_base = far.char, far.map
  screen.main.bg1 = true
end
```
