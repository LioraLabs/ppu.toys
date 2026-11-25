---
id: keyed-draws-and-keep
kind: recipe
title: A keyed draw and group:keep skip a sprite that did not move; a keep group is a sprite HUD
codes: []
fixture: twinstar
---
A draw with `key = n` owns the same sprite slots from frame to frame. A frame where that sprite did not move
or change can call `group:keep(n)` instead of drawing it: its sprites stay exactly as they were, and nothing
is rewritten. Track whether it moved yourself (a `moved` flag set by every change) and redraw on the first
frame it shows again after being hidden.

A group made with `{keep = true}` keeps everything it drew until it is drawn again: life icons, a sprite
score. Draw it only when what it shows changes; move a sprite it no longer shows off screen (`y = -16`).
See [[sprites.group]] and [[sprite-groups-stack-in-creation-order|stacking]].

```luau
local _ships = dma("ships", {palette = "ships_pal", char = 0x4000, pal = 0})
local bits = dma("bits", {palette = "bits_pal", char = 0x4800, pal = 6})
local LIFE = sprites.art({{size = 8, columns = 1, cells = {3}}}, bits)
local icons = sprites.group({keep = true})
local lives = makeBounded(3, 0, 3)
local shown = makeBounded(0, 0, 3)

function init()
  mode = 1
  screen.main.obj = true
  obj.char_base = _ships.char
end

function frame(t, f)
  if pad.pressed.a then lives = math.max(0, lives - 1) end
  if lives ~= shown then
    for i = 1, 3 do
      icons:draw(LIFE, 4 + i * 8, if i <= lives then 12 else -16, {key = i})
    end
    shown = lives
  end
end
```
