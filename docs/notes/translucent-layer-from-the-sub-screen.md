---
id: translucent-layer-from-the-sub-screen
kind: recipe
title: A layer is translucent when it is on the main screen, another is on the sub screen, and colour math halves their sum
codes: []
---
The PPU builds a main and a sub screen from the layers each lists (`screen.main`, `screen.sub`). Only
the main screen is seen; a layer on the sub screen is shown only as what colour math blends in. For a
see-through layer, put it on the main screen and the layer behind it on the sub screen, turn colour
math on for the front layer (`color.on.bg1`), take the sub screen as the addend, and set `color.half`
so the sum is a 50/50 blend rather than a brighter one. Layers not turned on in `color.on` stay opaque;
the backdrop and sprites have their own entries (`color.on.backdrop`, `color.on.obj`). Water over a
shore, fog and glass are this. Where the sub screen is empty, the front layer blends with the fixed colour (`color.fixed`) at full strength, unhalved, so fill the sub screen everywhere the translucent layer can be.

```luau
function init()
  mode = 1
  screen.main.bg1 = true
  screen.sub.bg2 = true
  color.on.bg1 = true
  color.addend = "sub"
  color.op = "add"
  color.half = true
end

function frame(t, f)
end
```
