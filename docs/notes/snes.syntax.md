---
id: snes.syntax
kind: diagnostic
title: Source that does not parse, or nests more than 64 levels deep, is refused before anything builds
codes: [snes.syntax]
---
`error[snes.syntax]` means a file does not parse, or nests blocks and expressions more than 64
levels deep, past what the build can read safely. The message gives the line. Fix the syntax it
names; for deep nesting, split a long expression with locals and move inner blocks into helper
functions, which also reads better.

```luau
local function inside(x, y)
  return x > 8 and x < 248 and y > 8 and y < 216
end

function frame(t, f)
  local x, y = f % 256, f % 224
  if inside(x, y) then
    brightness = 15
  else
    brightness = 8
  end
end
```
