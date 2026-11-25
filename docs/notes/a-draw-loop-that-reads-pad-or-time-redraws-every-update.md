---
id: a-draw-loop-that-reads-pad-or-time-redraws-every-update
kind: gotcha
title: A loop that writes cells or sprites skips unchanged entries only when it reads plain state; t, f, pad or a call in it redraws everything
codes: [snes.redraw]
fixture: title
---
A cartridge reruns a loop that writes tilemap cells or sprites only for the entries whose inputs changed
([[snes.redraw]]). That needs inputs it can compare: tables and variables. A body that reads `t`, `f` or
`pad`, calls a helper, plays a sound, loads art, breaks or returns, or writes a table or lasting variable runs
in full every update, silently. Read the time or the pad into a variable before the loop, and keep the loop's
body to arithmetic over its inputs and the writes.

```luau
local font = dma("font", {palette = "font_pal", char = 0x5000, pal = 112})
local cells = {makeBounded(0, 0, 255), makeBounded(0, 0, 255), makeBounded(0, 0, 255), makeBounded(0, 0, 255)}
local phase = makeBounded(0, 0, 3)

function init()
  mode = 1
  bg[1].char_base, bg[1].map_base = font.char, 0x7c00
  screen.main.bg1 = true
end

function frame(t, f)
  phase = f // 8 % 4 -- read f here, not inside the loop
  for i = 1, 4 do
    vram[0x7c00 + 64 + i - 1] = cells[i] + phase
  end
end
```
