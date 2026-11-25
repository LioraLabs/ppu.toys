---
id: a-key-holds-the-slots-of-its-first-art
kind: gotcha
title: A sprite key keeps as many slots as the first art drawn with it, so a bigger art on the same key is an error
codes: [snes.sprites]
fixture: twinstar
---
The first draw of a key in a group reserves that key's slots: one per sprite cell of the art it was drawn
with. A later draw of the same key with an art of more cells fails with `sprites key n has N slots, from its
first art`. Draw each key first with its largest art, or give the big form its own key. An
[[sprites.animation|animation]] counts as its largest art, so a walk cycle and a jump pose in one animation
share one key safely.

```luau
local _ships = dma("ships", {palette = "ships_pal", char = 0x4000, pal = 0})
local bits = dma("bits", {palette = "bits_pal", char = 0x4800, pal = 6})
local SMALL = sprites.art({{size = 8, columns = 1, cells = {3}}}, bits)
local WIDE = sprites.art({{size = 8, columns = 2, cells = {2, 3}}}, bits)
local hero = sprites.group()
local big = makeBounded(0, 0, 1)

function init()
  mode = 1
  screen.main.obj = true
  obj.char_base = _ships.char
end

function frame(t, f)
  if pad.pressed.a then big = 1 - big end
  -- Two keys: key 1 holds the wide form's two slots, key 2 the small form's one.
  if big == 1 then hero:draw(WIDE, 120, 100, {key = 1}) else hero:draw(SMALL, 120, 100, {key = 2}) end
end
```
