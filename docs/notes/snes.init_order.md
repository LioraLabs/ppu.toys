---
id: snes.init_order
kind: diagnostic
title: File-level code runs top to bottom once at power-on, so globals and helpers come before the code that uses them
codes: [snes.init_order]
---
`error[snes.init_order]` means power-on code reads a global before it is given a value, calls a helper before its
definition runs, or declares a global inside an `if` or loop at file level. A cartridge lays out its power-on code in
the order of the file and keeps each global at a fixed place, so it needs every global declared unconditionally and
every helper defined before a file-level call reaches it.

Declare globals first at file level, define helpers next, and call them last, or call them from `init()`, which runs
after the whole file. A value that depends on a condition starts with a plain value and is changed after.

```luau
local level = makeBounded(1, 1, 8)

local function setup()
  brightness = 15
  bg[1].scroll.x = level * 8
end

function init()
  setup()
end

function frame(t, f)
  bg[1].scroll.y = f % 256
end
```
