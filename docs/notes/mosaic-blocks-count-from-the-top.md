---
id: mosaic-blocks-count-from-the-top
kind: gotcha
title: Mosaic blocks count down from the top of the screen, and a new size starts a fresh row of blocks
codes: []
---
`mosaic` (0 to 15, block sides of 1 to 16 pixels) holds one sampled pixel across a square block on every
background whose `mosaic` flag is on. The blocks count from the top of the screen, as on hardware.
Changing the size part way down the screen starts a new row of blocks on that line; rewriting the same
size, or switching a layer's flag on or off, keeps the count running. A pulse that changes the size
every frame therefore keeps whole blocks; one that changes it inside a scanline callback leaves a
partial row where the size changed.

```luau
function init()
  mode = 1
  screen.main.bg1 = true
  bg[1].mosaic = true
end

function frame(t, f)
  mosaic = f // 4 % 16
end
```
