---
id: alias-pad-inside-frame
kind: gotcha
title: "`local p = pad` at file level is refused, because it would hold the controller as it was at power-on"
codes: [snes.unsupported]
---
`pad` is replaced by a new reading every update, so a file-level alias would keep the first one forever. A
cartridge refuses `local p = pad` outside a function. Alias it inside `frame()` or a helper, or read
`pad.left` where you need it.

```luau
local x = makeBounded(128, 0, 255)

function frame(t, f)
  local p = pad -- inside frame: this update's reading
  if p.left then x = math.max(0, x - 1) end
  if p.right then x = math.min(255, x + 1) end
end
```
