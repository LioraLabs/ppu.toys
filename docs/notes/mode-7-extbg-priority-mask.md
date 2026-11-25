---
id: mode-7-extbg-priority-mask
kind: recipe
title: EXTBG gives every Mode 7 pixel a priority from a black and white mask, shown by BG1 and BG2 together
codes: []
---
An `m7` source with `"extbg": true` in its options and a same-sized black and white `priority_file`
splits the plane in two: the mask's bright pixels are the high-priority ones, so a sprite can pass
under a bridge's deck or a tree's crown and over the ground. EXTBG keeps 7 bits of
colour (127 colours and transparent) in place of 8. Turn it on with `m7.extbg = true`; the plane is
shown by `bg1`. Without EXTBG, every sprite draws in front of the Mode 7 plane whatever its `prio`; `prio` then orders sprites only among themselves.

```luau
function init()
  mode = 7
  m7.extbg = true
  screen.main.bg1, screen.main.obj = true, true
end

function frame(t, f)
end
```
