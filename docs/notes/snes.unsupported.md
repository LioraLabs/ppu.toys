---
id: snes.unsupported
kind: diagnostic
title: A Lua feature with no cartridge form is refused, and this note gives the rewrite
codes: [snes.unsupported]
fixture: title
---
`error[snes.unsupported]` means the code uses something a cartridge has no representation for. The
message names what was refused. Common ones: a string built from a run-time number
(`"SCORE " .. score`) instead of a [[@ppu/hud|hud]] field; a table of functions instead of an `if`
over a state; a value that is sometimes a number and sometimes a boolean; a boolean compared with
nil. Refusals of `dma()` and of the old `hud_text` calls carry their own codes, `snes.dma` and
`snes.hud`; so do refused sounds ([[snes.sound]]: `bank`, `sfx`, voice reads, echo that is not constant) and table
literals with no layout ([[snes.table]]: mixed number types, computed keys).

Smaller rewrites that recur:

- A loop's step is a constant integer other than zero, and a loop runs at most 65,536 times. Cover
  a bigger range over several updates with a base that moves, or draw it from an image with `dma()`.
- A shift's amount is a constant from 0 to 30; for a variable count write an `if` chain of constant
  shifts. A right shift needs a value proven not to go below zero: keep it in a `makeBounded`
  state, or write `x // 4`.
- `goto continue` has no form: wrap the rest of the loop body in `if not skip then ... end`.
- `/` may give a fraction. Use `//` on whole numbers, or make the value fixed point with
  `makeFixed` and `floor()` it before it is a position.
- The time `t` is a fraction: count updates with `f`, which is 60 a second on NTSC and 50 on PAL.
- A helper takes fixed parameters, never `...`, and a Lua toy's `init` and `frame` do the same.
- A numeric `for` counter stays within -32768..32767, and so do its end and end plus step (or it counts
  0..65535 by 1): loop over the smaller range and scale the counter in the body,
  `for i = 0, 10 do local x = i * 7000 end`.
- Wrapping arithmetic written as `( ... ) % 2147483648` has no form: keep the terms under 2^31, as in
  `seed = (seed * 25173 + 13849) % 65536`, and take the digits you need from that.
- A function (`frame()` included) is one piece of code and is never split; one top-level statement of
  setup code is too. Each must fit a 32 KiB ROM bank, so move part of its body into functions it calls.
- `(wide value)` means a value can grow past what its code handles: bound it with `makeBounded`, or
  keep it in range with `% N` where it changes (a counter that only grows, drawn in a loop, is the usual case).

```luau
local hud = require("@ppu/hud")
local font = dma("font", {palette = "font_pal", char = 0x5000, pal = 112})
local HUD = {map = 0x7c00, width = 32, height = 28, palette = 7}
local score = makeBounded(0, 0, 999)
local shown

function init()
  mode = 1
  bg[1].char_base, bg[1].map_base = font.char, HUD.map
  screen.main.bg1 = true
  hud.text(HUD, 11, 13, "SCORE")
  shown = hud.field(HUD, 18, 13, 3)
end

function frame(t, f)
  if pad.pressed.a then score = math.min(score + 1, 999) end
  shown:set(score) -- not hud.text(HUD, 11, 13, "SCORE " .. score)
end
```
