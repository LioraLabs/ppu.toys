---
id: snes.hdma
kind: diagnostic
title: An hdma() callback writes registers per row from its row number, constants and the frame's values, called from frame()
codes: [snes.hdma]
---
`error[snes.hdma]` means an [[hdma|hdma()]] call is outside the shape a cartridge turns into a scanline table. A
cartridge runs the rows at the `hdma()` call, from a table filled ahead; the preview runs them after `frame()` returns.
The shape that means the same on both:

- called from `frame()` or a helper only `frame()` calls, never at file level or from `init()`;
- first and last rows are literal numbers in 0..223, first no larger than last; schedules do not overlap;
- the callback writes registers from `y`, constants and values set before the call; it does not return, call helpers
  that read registers, write VRAM, store game state, or write `cgram[]` at an index that changes per row;
- each register it writes or reads is set before the `hdma()` call and not touched after it in that frame;
- at most 7 register groups change per row (each layer's scroll is 1, window edges 1, `color.fixed` 2, each palette
  entry 1): set the ones that hold all frame once in `frame()` instead.

When the callback has one moving input, [[bake-scanline-effects|bake it]] instead.

```luau
function frame(t, f)
  local shift = f % 16
  bg[2].scroll.y = 0
  hdma(0, 111, function(y)
    bg[1].scroll.x = (y // 8 + shift) % 16
  end)
end
```
