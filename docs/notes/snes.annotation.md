---
id: snes.annotation
kind: diagnostic
title: A `---@snes` comment the cartridge cannot attach to a declaration is refused; Luau states its ranges in code instead
codes: [snes.annotation]
---
`error[snes.annotation]` means a `---@snes range`, `fixed` or `cap` comment does not sit on the line directly above
the declaration it describes, names a value it cannot describe (a record table, a string, an alias), or gives a pool
fewer slots than its constructors need. A cartridge reads these comments as sizes, so one it cannot attach is an error,
not a hint it can skip.

A Luau toy needs none of them. Delete the comment and say the same thing in the code: [[makeBounded]] for a whole
number's range, [[makeFixed]] for a fraction's range and step, and a length check before each append for a list's
size (`if #list < 8 then ... end`), which the cartridge counts as the list's capacity.

```luau
-- Was: ---@snes range score 0 9999 above local score = 0
local score = makeBounded(0, 0, 9999)
local speed = makeFixed(0, -4, 4, 16)
local shots: {{x: number}} = {}

function frame(t, f)
  if pad.a and #shots < 8 then
    shots[#shots + 1] = {x = makeBounded(0, 0, 255)}
    score = math.min(score + 1, 9999)
  end
  speed = math.clamp(speed + 0.25, -4, 4)
end
```
