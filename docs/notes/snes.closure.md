---
id: snes.closure
kind: diagnostic
title: A function stored as a value is called as a statement with no arguments and no result
codes: [snes.closure]
---
`error[snes.closure]` means a function kept in a variable or table field is used in a way a cartridge cannot keep.
It stores which function it is and the values it captured, nothing more, so a stored function:

- takes no parameters and returns nothing; call it as a statement, `on_hit()`, never inside an expression;
- captures only values that are not assigned again after it is made (copy a changing one into a fresh local first);
- holds no function written inside it: move that one to a file-level helper.

When it needs an input, write the input into a variable it reads; when it computes a result, write the result into a
variable or field instead of returning it.

```luau
local flash = makeBounded(0, 0, 15)
local strength = makeBounded(8, 0, 15)

local handlers = {on_hit = function()
  flash = strength
end}

function frame(t, f)
  if pad.a then handlers.on_hit() end
  if flash > 0 then flash = flash - 1 end
  brightness = 15 - flash
end
```
