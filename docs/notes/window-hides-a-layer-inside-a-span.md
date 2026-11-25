---
id: window-hides-a-layer-inside-a-span
kind: recipe
title: A window hides a layer inside its span from lo to hi, or outside it when inverted, once the layer is masked on the screen
codes: []
---
A window is a horizontal span, `win.w1.lo` to `win.w1.hi` inclusive, that decides where a layer is
hidden. Three switches make it act: the layer opts in (`win.bg1.w1`), the mask applies on the main
screen (`win.bg1.main`; `.sub` for the sub screen), and `win.bg1.invert` chooses the outside of the
span instead of the inside. Sprites and the colour window have their own entries, `win.obj` and
`win.color` ([[colour-math-panel-in-a-window]]). A span whose `lo` is above its `hi` (`255, 0`) is
empty: it hides nothing, or, inverted, everything, which is how a line is switched on or off. Set the edges per line in an
[[hdma]] callback for a shape that is not a rectangle ([[iris-wipe-routine]]).

```luau
function init()
  mode = 1
  screen.main.bg1 = true
  win.w1.lo, win.w1.hi = 48, 208
  win.bg1.w1, win.bg1.main = true, true
  win.bg1.invert = false -- true keeps the layer inside the span and hides the rest
end

function frame(t, f)
end
```
