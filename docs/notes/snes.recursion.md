---
id: snes.recursion
kind: diagnostic
title: A cartridge has no call stack for recursion, so a recursive helper becomes a bounded loop
codes: [snes.recursion]
---
`error[snes.recursion]` means a helper calls itself, directly or through other helpers. A cartridge
gives every helper fixed storage for its locals, so a helper cannot be running twice at once.
Write the recursion as a loop with a literal bound (`for step = 1, 8 do ... end`), keeping its state
in locals or file-level variables, or in a table used as a stack.

```luau
-- Was: local function digits(n) if n < 10 then return 1 end return 1 + digits(n // 10) end
local function digits(n)
  local count = 1
  local rest = n
  for _ = 1, 5 do
    if rest >= 10 then
      rest = rest // 10
      count = count + 1
    end
  end
  return count
end

function frame(t, f)
  brightness = 10 + digits(f % 10000)
end
```
