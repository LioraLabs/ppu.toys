---
id: scalars-are-words-and-table-cells-shrink-to-bytes
kind: concept
title: A variable is 2 bytes on a cartridge, a table cell bounded 0..255 is 1, and a value past 65535 or unbounded is 4
codes: [snes.wram]
---
Work RAM and speed follow a value's range. A single variable takes 2 bytes whatever its range, because the
console's processor works in 16 bits. A table cell bounded to 0..255 takes 1 byte, one within -32768..32767 or
0..65535 takes 2, and a cell or variable with a wider range, or none proven, takes 4, and every sum or compare
on it does twice the work. Bound a score to 65535 or less unless it truly needs more, and keep tile ids,
flags and counts in tables as 0..255 values with [[makeBounded]].

```luau
local score = makeBounded(0, 0, 65535) -- 2 bytes; a bound of 99999 would make it 4
local kinds = {makeBounded(0, 0, 255), makeBounded(0, 0, 255), makeBounded(0, 0, 255), makeBounded(0, 0, 255)} -- 4 bytes, one a cell

function frame(t, f)
  if pad.pressed.a then score = math.min(65535, score + 10) end
end
```
