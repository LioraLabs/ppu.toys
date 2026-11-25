---
id: snes.require
kind: diagnostic
title: require takes a literal "./name" for a project file or "@ppu/name" for an engine library, with no cycles
codes: [snes.require]
---
A cartridge links every module ahead of time, so it must see which file each `require` names and an
order to load them in. `error[snes.require]` comes in four forms: a path that is neither `"./name"`
nor `"@ppu/name"`; no file `name.luau` in the project; an engine library required by a name that is
not written out as a literal string; and a cycle, two modules that require each other.

Write the path as a string literal at the top of the file and leave off the extension. Break a cycle
by moving what both modules share into a third that neither of them requires back.

```luau
local blink = require("@ppu/blink")

function frame(t, f)
  brightness = if blink.on(f, 8) then 15 else 6
end
```
