---
id: method-calls-need-one-helper-per-field
kind: rule
title: "`r:update()` works on a cartridge when every constructor of that record stores the same file-level helper in the field"
codes: [snes.unsupported]
---
A record method call is a plain call of a helper on a cartridge: the field holds no value and costs no RAM.
That needs one answer to "which helper": every constructor of the record sets the field to the same
file-level function, declared above it. A constructor that stores another function or leaves the field out,
a read of the field as a value (`r.update(r)` with a dot is one), or a store to it is refused. For several
behaviours, keep a number `kind` field and branch on it inside the one helper.

The call has to reach the helper by its bare name from where it stands: a local or parameter with
the same name hides it, a `local function` in another file is private to that file, and a `local
function` declared below the constructor is not yet the helper. A `lookup` function is a table in
ROM, so it cannot be a method either; call it by its name, `sq(x)`.

```luau
local function update(e)
  if e.kind == 1 then e.x = (e.x + 1) % 256 else e.x = (e.x + 255) % 256 end
end

local function enemy(kind)
  return {kind = kind, x = makeBounded(0, 0, 255), update = update}
end

local a, b = enemy(1), enemy(2)

function frame(t, f)
  a:update()
  b:update()
end
```
