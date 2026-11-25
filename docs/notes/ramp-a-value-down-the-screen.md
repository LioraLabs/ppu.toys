---
id: ramp-a-value-down-the-screen
kind: recipe
title: ramp_color and ramp_int with literal keys inside hdma() become rows in ROM, so a gradient costs no per-line work
codes: []
---
Inside an [[hdma]] callback, `ramp_int(keys, y)` and `ramp_color(keys, y)` read a curve at the row:
`keys` is a table of `{row, value}` pairs sorted by row, the value holds before the first key and after
the last, and a key may add an easing (`"smooth"`, `"ease-in"`, `"ease-out"`, `"step"`) for the segment
that follows. Write the keys as literals and a cartridge folds the curve into a table filled at build
time instead of calling the helper per line. Use `ramp_int` for integer registers (scroll, window edges)
and `ramp_color` for a packed 15-bit colour, whose channels are interpolated separately. A curve that also
reads game state is [[snes.hdma_live|live]]: put that state in a [[bake-scanline-effects|bake]].

Colours here are packed BGR555 literals, `0x3082` for `rgb(16, 32, 96)` and `0x221e` for
`rgb(240, 128, 64)`: a dusk sky on the backdrop entry, and a sway in the second layer.

```luau
function init()
  mode = 1
  screen.main.bg1, screen.main.bg2 = true, true
end

function frame(t, f)
  hdma(0, 223, function(y)
    cgram[0] = ramp_color({{0, 0x3082}, {223, 0x221e, "smooth"}}, y)
    bg[2].scroll.x = ramp_int({{0, 0}, {112, 12, "smooth"}, {223, 0}}, y)
  end)
end
```
