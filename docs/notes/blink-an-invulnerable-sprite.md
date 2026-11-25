---
id: blink-an-invulnerable-sprite
kind: recipe
title: An invulnerable sprite blinks with blink.on over its safe-frames counter
codes: []
fixture: twinstar
---
After a hit, a counter of safe frames runs down, and the sprite shows only on the "on" half of a blink:
`blink.on(safe, 4)` from [[@ppu/blink]] is true for 4 frames, false for 4, true at 0 so the sprite is
solid again when the counter ends. Skip the draw on off frames; with [[keyed-draws-and-keep|keyed draws]],
mark the sprite to be drawn afresh when it shows again. The same counter is what
[[knockback-then-invulnerability|the hurt rule]] checks.

```luau
local blink = require("@ppu/blink")
local sheet = dma("ships", {palette = "ships_pal", char = 0x4000, pal = 0})
local SHIP = sprites.art({{size = 16, columns = 1, cells = {0}}}, sheet)
local ships = sprites.group()
local safe = makeBounded(0, 0, 120)

function init()
  mode = 1
  screen.main.obj = true
  obj.char_base = sheet.char
end

function frame(t, f)
  if pad.pressed.a and safe == 0 then safe = 120 end
  safe = math.max(0, safe - 1)
  if blink.on(safe, 4) then
    ships:draw(SHIP, 128, 104)
  end
end
```
