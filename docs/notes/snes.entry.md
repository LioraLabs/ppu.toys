---
id: snes.entry
kind: diagnostic
title: A cartridge starts from frame(t, f), so a toy defines it at file level or adds scenes
codes: [snes.entry]
---
`error[snes.entry]` means the cartridge build found no `frame(t, f)` to call each update. Define `function frame(t, f)`
at file level in main.luau (not `local`, not inside another function); `init()` is optional setup. A game of screens
uses [[scenes.add]] in main.luau instead: each scene file returns its own `init` and `frame`.

```luau
local x = makeBounded(0, 0, 255)

function init()
  brightness = 15
end

function frame(t, f)
  x = (x + 1) % 256
  bg[1].scroll.x = x
end
```
