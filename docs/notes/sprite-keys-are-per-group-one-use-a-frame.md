---
id: sprite-keys-are-per-group-one-use-a-frame
kind: gotcha
title: A sprite key is an integer 0 to 127 owned by one group, and each key is drawn or kept once a frame
codes: []
fixture: twinstar
---
Keys are a group's own: the same number in two groups is two sprites. A key must be a whole number from 0 to 127. A frame
may draw a key once or keep it once, never both, and never twice; `keep` of a key that group never drew is an error. The first draw
of a key gives it a block of the group's slots as large as that art's largest level, so a later draw with larger art is refused: give the key the
biggest art first. A key a frame neither draws nor keeps is hidden, and keeping it then keeps it hidden until it is drawn again.
A first draw that needs more slots than its group has left is an error too: widen the group with `slots`. See
[[keyed-draws-and-keep]] for what a key buys.

```luau
local sheet = dma("ships", {palette = "ships_pal", char = 0x4000, pal = 0})
local SHIP = sprites.art({{size = 16, columns = 1, cells = {0}}}, sheet)
local ships = sprites.group()
local x = makeBounded(40, 0, 240)

function init()
  mode = 1
  screen.main.obj = true
  obj.char_base = sheet.char
end

function frame(t, f)
  -- Key 1 drawn when it moved, kept when it did not: never both in one frame.
  if f % 4 == 0 then
    x = (x + 4) % 240
    ships:draw(SHIP, x, 104, {key = 1})
  else
    ships:keep(1)
  end
end
```
