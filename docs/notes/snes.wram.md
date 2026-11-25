---
id: snes.wram
kind: diagnostic
title: A cartridge has 128 KiB of work RAM, so tables are sized by their keys' ranges and constant data lives in ROM
codes: [snes.wram]
---
Every variable, table and record that lasts between frames lives in the cartridge's work RAM, and a
table gets a cell for every key it can be indexed by. `error[snes.wram]` means the state does not
fit: a table indexed over a range too wide for a table (the message gives the range), or all the
state together past the room. Warnings with the same code name the largest users.

Bound every table key to the range it really needs, wrapping or clamping it. Give [[makeBounded]]
and [[makeFixed]] state honest, tight bounds. Move data that never changes into a [[lookup]], which
lives in ROM. Only tables can move to the upper part of work RAM: single variables, each table's length and the runtime's own state stay in the low 24 KiB, so a game of many separate variables can fill low RAM while room is left above. The sprite buffer and the scanline tables ([[snes.bank7f]]) have regions of their own and do not count here.

```luau
local trail = {}
for i = 1, 64 do trail[i] = 0 end

function frame(t, f)
  trail[f % 64 + 1] = f % 256 -- was trail[f], a key that grows without end
  bg[1].scroll.x = trail[(f + 1) % 64 + 1]
end
```
