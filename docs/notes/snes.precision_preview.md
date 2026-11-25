---
id: snes.precision_preview
kind: diagnostic
title: A program with fixed-point state is checked with the cartridge's arithmetic, which takes a smaller set of shapes
codes: [snes.precision_preview]
---
When a program has [[makeFixed]] state, the check runs it with the cartridge's own fixed-point
arithmetic, so the preview and the cartridge compute the same numbers. That mode reads a smaller
set of statement shapes, and `error[snes.precision_preview]` names one outside it, at its line,
with the rewrite. Common ones: `frame` must be declared `frame(t, f)` (time comes from the frame
counter), loops need whole-number bounds (floor a fractional limit first), a helper is a plain
named function, and a value keeps one type.

```luau
local angle = makeFixed(0, 0, 255, 16)

function frame(t, f)
  angle = (angle + 0.5) % 256
  for i = 1, 4 do
    bg[i].scroll.x = math.floor(angle) + i
  end
end
```
