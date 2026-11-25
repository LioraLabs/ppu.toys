---
id: two-windows-combine-with-logic
kind: recipe
title: Two windows on one layer combine with win.<layer>.combine: OR, AND, XOR or XNOR
codes: []
---
Turn on both windows for a layer (`win.bg1.w1` and `win.bg1.w2`) and `win.bg1.combine` sets how their
two masks merge: `"OR"` hides where either window is, `"AND"` only where both are, `"XOR"` where
exactly one is (the overlap stays visible), `"XNOR"` the opposite. Two overlapping spans and XOR hide the
two side bands and leave the overlap visible; two disjoint spans and OR make two panels. Each layer (`bg1` to `bg4`, `obj`,
`color`) has its own `combine`. The setting does nothing until the mask is on for the screen
([[window-hides-a-layer-inside-a-span]]).

```luau
function init()
  mode = 1
  screen.main.bg1 = true
  win.w1.lo, win.w1.hi = 48, 208
  win.w2.lo, win.w2.hi = 96, 160
  win.bg1.w1, win.bg1.w2 = true, true
  win.bg1.combine = "XOR"
  win.bg1.main = true
end

function frame(t, f)
end
```
