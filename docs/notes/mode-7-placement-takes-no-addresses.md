---
id: mode-7-placement-takes-no-addresses
kind: gotcha
title: A Mode 7 source is placed with no addresses, and its plane owns VRAM 0x0000..0x3fff and CGRAM from 1
codes: []
fixture: twinstar
---
Mode 7's layout is fixed: tiles and map interleave at VRAM 0x0000..0x3fff. So [[dma]] of an `m7`
source takes its `palette` and nothing else: a `char`, `map` or `pal` option is an error naming the
source. Its colours go to CGRAM from entry 1. Place everything else around it: sprites at 0x4000 or
above, a font and text map higher still, and other palettes after the plane's colours (the font at 112,
sprites in the sprite half from 128).

```luau
local _deck = dma("deck", {palette = "deck_pal"})
local _ships = dma("ships", {palette = "ships_pal", char = 0x4000, pal = 0})

function init()
  mode = 7
  screen.main.bg1, screen.main.obj = true, true
  obj.char_base = 0x4000
end

function frame(t, f)
  m7.cx, m7.cy = 512, 512
  bg[1].scroll.x, bg[1].scroll.y = 384, 399
end
```
