---
id: snes.hdma_alike
kind: diagnostic
title: A register an hdma() callback writes with the same value on every row belongs in frame()
codes: [snes.hdma_alike]
---
The warning `This scanline schedule writes ... with the same value on every row` means part of an [[hdma|hdma()]]
callback does not depend on the row. The cartridge still packs that value into the schedule's table once per row,
every frame, and the table takes work RAM. Set it once in `frame()`, before the `hdma()` call: the register keeps it
for the whole frame. Keep in the callback only what changes with `y`, and narrow the rows to where it changes.

```luau
function frame(t, f)
  -- Was: both lines inside the callback over rows 0..223.
  bg[2].scroll.x = f % 256
  hdma(0, 223, function(y)
    bg[1].scroll.x = y // 4
  end)
end
```
