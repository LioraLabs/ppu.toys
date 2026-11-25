---
id: snes.frontend
kind: diagnostic
title: A few Luau forms are outside what ppu runs; each has a plain equivalent
codes: [snes.frontend]
---
`error[snes.frontend]` names a Luau form ppu does not run, in the preview or on a cartridge. Each has a plain
replacement:

- interpolated strings (`` `score {n}` ``): use `..`, or `require("@ppu/hud")` to draw text and numbers;
- `const` declarations and functions, attributes (`@native`), condition bindings: plain `local` and `function`;
- an `if ... then a else b` expression whose then-value may be `false` or `nil` and whose condition calls a function:
  write an `if` statement;
- a compound assignment (`+=`) to a target that calls a function (`t[f()] += 1`): bind the called part to a local first;
- `require("@ppu/name")` of a library that does not exist: the message lists the ones that do.

```luau
local counts = {0, 0, 0, 0}

local function slot(f: number): number
  return f % 4 + 1
end

function frame(t, f)
  -- Was: counts[slot(f)] += 1
  local i = slot(f)
  counts[i] = (counts[i] + 1) % 256
end
```
