---
id: snes.mixed_comparison
kind: diagnostic
title: A boolean compared with a number is never equal, so the comparison is a constant
codes: [snes.mixed_comparison]
---
The warning `This == compares a boolean with a number` means one side of `==` or `~=` is `true`/`false` and the other
a number. Lua never finds them equal: `==` is always false and `~=` always true, in the preview and on the cartridge.
That is rarely what the code meant. Compare like with like: test a boolean directly, or store numbers and compare
numbers.

```luau
local solid = {true, false, true}
function frame(t, f)
  local i = f % 3 + 1
  -- Was: if solid[i] == 1 then  (always false)
  if solid[i] then
    bg[1].scroll.x = i
  end
end
```
