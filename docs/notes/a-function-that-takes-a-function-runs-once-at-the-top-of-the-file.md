---
id: a-function-that-takes-a-function-runs-once-at-the-top-of-the-file
kind: rule
title: A function passed as an argument must be a file-level or module function, and the callee gets a copy for each one
codes: [snes.unsupported]
---
A cartridge has no function values. A call that hands a function to another function
(`slide_x(blocked, x, vx)`) must pass the function itself: a `local function` of the file or a
module's function, by name. Each distinct function a helper is given gets its own copy of the helper
that calls it directly, so two scenes can share one helper and each pass its own `blocked`. A
variable, a table field or an `if` expression that picks a function is refused: call the helper once
for each function instead. The helper does not assign the parameter that receives the function.

A call made as a statement at the top level of the file is written out in place instead, so the
tables the callee builds are file-level tables. There the callee has named parameters (no `...`),
returns one value as its last statement and nowhere else, does not call itself, has no label or
`goto`, and builds no [[snes.bounded]] record: build that in a file-level factory and pass it in.
Every name it uses means the same at the call as where it is defined, so rename a local that hides
one, or pass the value as a parameter.

```luau
local function double(i) return i * 2 end
local function triple(i) return i * 3 end

local function total(n, step)
  local sum = 0
  for i = 1, n do sum = sum + step(i) end
  return sum
end

local x = makeBounded(0, 0, 255)

function frame(t, f)
  x = (x + total(4, double) + total(2, triple)) % 256
end
```
