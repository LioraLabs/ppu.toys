---
id: a-module-is-a-table-of-named-fields-read-by-field
kind: rule
title: A module is a table of named fields, and other files read it as Module.field
codes: [snes.unsupported]
---
A cartridge links modules ahead of time, so each export has to be a name it can follow. A module
returns one table of named fields, `return {step = step}`, or builds the table as `local M = {size = 8}`
with `M.name = local_name` and `function M.name(...)` statements and ends with `return M`. A field
starts as a local's name or a literal, each exported local is declared once on a line of its own, and
only a module that is required returns at its top level.

Other files read it as `Module.field`: a field that does not exist, a method call or a computed field
is refused. The module itself is not a value. Name it with a local declared once and never reassigned,
`local s = State`, or pass it to a local function whose every call passes that same module; to pick
between modules, branch on the choice.

```luau
-- main.luau
local Shared = require("./shared")

local n = makeBounded(0, 0, 3)

function frame(t, f)
  n = Shared.bump(n)
end

-- shared.luau
local limit = 3

local function bump(n)
  return math.min(n + 1, limit)
end

return {limit = limit, bump = bump}
```
