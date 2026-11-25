---
id: snes.function
kind: diagnostic
title: Each function name is defined once, because a cartridge calls every function at a fixed address
codes: [snes.function]
---
`error[snes.function]` means two definitions share a name in one file's scope: two `function update()`, or a
[[lookup]] and a function with the same name. A cartridge gives each function one fixed place in ROM, so a later
definition cannot replace an earlier one the way it does in Lua. Keep one definition and give the other its own name.
To choose between behaviours at run time, branch inside one function, or keep a [[snes.closure|stored function]] in a
variable.

```luau
local speed = makeBounded(1, 1, 4)

local function update_slow()
  bg[1].scroll.x = bg[1].scroll.x % 256
end

local function update_fast()
  speed = 4
end

function frame(t, f)
  if pad.a then update_fast() else update_slow() end
end
```
