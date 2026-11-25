---
id: snes.inferred
kind: diagnostic
title: A list's size the cartridge inferred from its appends must still hold for every append
codes: [snes.inferred]
---
`snes.inferred` reports a list or pool whose size the cartridge proved from the code but could not keep. As a warning
(`was not applied`, `not inferred until the errors above are fixed`), the list keeps its default size and a later fix
may restore it: fix the errors above first. As an error (`no longer holds once the pool is sized`), some append is not
covered by the proof.

The proof reads appends written behind a length check, `if #list < N then list[#list + 1] = v end`, and copies of a
bounded list. Write every append to the list in that form, with one N, so each one keeps inside the same size; an
append in a form the proof does not recognise leaves the whole list unproven. See [[snes.capacity]].

```luau
local MAX = 12
local sparks: {{x: number}} = {}

local function spawn(x: number)
  if #sparks < MAX then
    sparks[#sparks + 1] = {x = makeBounded(x, 0, 255)}
  end
end

function frame(t, f)
  if pad.a then spawn(f % 256) end
  if pad.b then spawn(128) end
end
```
