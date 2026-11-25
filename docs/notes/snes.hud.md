---
id: snes.hud
kind: diagnostic
title: The cartridge has no hud_text, hud_number or hud_clear, and a HUD call stops on a field outside the map
codes: [snes.hud]
fixture: title
---
`error[snes.hud]: ...` comes from a HUD call that does not fit: a [[@ppu/hud]] call, or one of the
deprecated built-in calls. A cartridge refuses `hud_text`, `hud_number` and `hud_clear` outright: its
HUD is [[@ppu/hud]] ([[hud-is-retained]]). The preview still runs them for older toys. Both stop on
the same mistakes:

- `HUD width/height must be 1..32 cells`, `HUD map is outside VRAM`: the map descriptor is wrong.
- `HUD field is outside its map`, `HUD cell coordinates must be integers`: the text or number runs
  past the map's edge, or `x`/`y` are fractions.
- `HUD character is absent from the printable ASCII font`: the text holds a character outside space to `~`.
- `HUD number width must be 1..10 cells`, `HUD number must be an integer in 0..2147483647`,
  `HUD palette must be 0..7`.
- `HUD field width must be 1..9 cells`, `HUD field value must be an integer that fits the field's
  width`, `HUD line text is wider than its cells`: a [[@ppu/hud]] field or line is too narrow for
  what it holds ([[hud-field-holds-whole-numbers-below-its-width]]).

The fix for all of them is the library: a label once in `init`, a field set every frame.

```luau
local hud = require("@ppu/hud")
local font = dma("font", {palette = "font_pal", char = 0x5000, pal = 112})
local HUD = {map = 0x7c00, width = 32, height = 28, palette = 7}
local score = makeBounded(0, 0, 999)
local field

function init()
  mode = 1
  bg[1].char_base, bg[1].map_base = font.char, HUD.map
  screen.main.bg1 = true
  hud.text(HUD, 2, 1, "SCORE")
  field = hud.field(HUD, 8, 1, 3)
end

function frame(t, f)
  if pad.pressed.a then score = math.min(999, score + 1) end
  field:set(score)
end
```
