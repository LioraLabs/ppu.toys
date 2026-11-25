---
id: a-keyed-animation-restarts-when-its-key-is-skipped
kind: gotcha
title: A keyed animation starts again from its first art when its key goes an update undrawn or is drawn with other art
codes: []
fixture: twinstar
---
An [[sprites.animation|animation]] drawn with a `key` plays on that key's clock, and the clock keeps running
only while the key is drawn with the same animation every update. Skip the key for one update (a blink that
leaves the sprite out) or draw it once with other art (a hurt flash) and the animation starts again at its
first art. For an animation that must keep its step through blinks, drive it yourself with `at`, a count of
video frames, from a timer you keep.

```luau
local _ships = dma("ships", {palette = "ships_pal", char = 0x4000, pal = 0})
local bits = dma("bits", {palette = "bits_pal", char = 0x4800, pal = 6})
local A = sprites.art({{size = 8, columns = 1, cells = {3}}}, bits)
local B = sprites.art({{size = 8, columns = 1, cells = {2}}}, bits)
local WALK = sprites.animation({A, B}, 8)
local actors = sprites.group()
local age = makeBounded(0, 0, 15)

function init()
  mode = 1
  screen.main.obj = true
  obj.char_base = _ships.char
end

function frame(t, f)
  age = (age + 1) % 16
  -- Blinks every other 4 frames, but the walk keeps its step: the clock is `age`, not the key's.
  if f // 4 % 2 == 0 then actors:draw(WALK, 120, 100, {at = age}) end
end
```
