---
id: a-pixel-passes-through-six-stages
kind: concept
title: A pixel passes through six stages in order, and each stage has its own fields to write
codes: []
---
The PPU draws no shapes: it reads tiles and sprites from memory, decides which pixel is in front,
optionally combines two colours and sends the result to the display. In order: [[graphics-come-from-sources|sources]]
become tiles, maps and palettes in memory; backgrounds (`bg[n]`) and sprites ([[sprites-group-draw-every-frame|groups]])
fetch pixels from it; `screen.main` and `screen.sub` decide which layers may appear; windows mask those
layers by horizontal region; colour math combines the winning main pixel with another colour; `brightness`
fades the finished image. When an effect is wrong, find the stage: a layer missing is usually
`screen.main`, a layer cut off a window, a tint colour math. Registers apply to the whole frame; the same
assignments inside an `hdma()` hook change per scanline ([[snes.hdma]]).

```luau
function init()
  mode = 1
  screen.main.bg1 = true
  screen.main.obj = true
end

function frame(t, f)
  brightness = 15
end
```
