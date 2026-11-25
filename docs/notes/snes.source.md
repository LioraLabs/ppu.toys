---
id: snes.source
kind: diagnostic
title: A source file has at most 65,535 lines, so long files and generated data are split or computed
codes: [snes.source]
---
`error[snes.source]` means one file has more than 65,535 lines: a cartridge records source
positions in 16 bits. Split the file in two, moving helpers or data into a second module that the
first requires. A file that long is usually generated data; a table of values a formula gives is
better as a [[lookup]], which the build computes instead of the source spelling every entry.

```luau
-- Was a 4,096-line table of heights; the build computes the same values.
local height = lookup({x = {0, 4095}}, function(x)
  return math.floor(96 + math.sin(x * math.pi / 512) * 32)
end)

function frame(t, f)
  bg[1].scroll.y = height(f % 4096)
end
```
