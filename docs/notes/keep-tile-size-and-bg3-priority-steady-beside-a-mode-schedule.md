---
id: keep-tile-size-and-bg3-priority-steady-beside-a-mode-schedule
kind: gotcha
title: Changing a tile size or BG3 priority while an hdma() callback writes mode rewrites every mode row, on two updates
codes: [snes.budget]
---
When a live [[hdma]] callback sets `mode` per line, the mode rows also carry the backgrounds' tile sizes and
BG3's priority. Change either from game state and a cartridge rewrites every one of those rows, about a tenth
of a frame on each of two updates. Set tile sizes and BG3 priority once in `init` or a scene's setup, and
change them only at a cut, or keep `mode` out of the callback.

```luau
function init()
  mode = 1
  bg3_priority = true -- set once, never from the game loop
  screen.main.bg1 = true
  screen.main.bg3 = true
end

function frame(t, f)
  hdma(0, 223, function(line)
    mode = if line < 160 then 1 else 0
  end)
end
```
