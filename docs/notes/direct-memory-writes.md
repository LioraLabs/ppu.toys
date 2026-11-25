---
id: direct-memory-writes
kind: rule
title: vram[] and cgram[] are for the few words code computes; art goes in a source
codes: []
---
`cgram[i] = rgb(r, g, b)` sets one colour of the 256 ([[cgram]], [[rgb]]), and `vram[addr] = word`
([[vram]]) writes words the program computes, such as a tilemap row it builds. Raw writes
land after sources and tilemap cells every frame, so they win where addresses overlap. Graphics a
person drew belong in a source placed with [[dma]]: filling VRAM with `vram[]` loops in `init()`
shows a console black for seconds.

```luau
function frame(t, f)
  cgram[0] = rgb(24, 16, 72) -- the backdrop colour
  brightness = 15
end
```
