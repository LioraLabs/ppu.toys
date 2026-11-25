---
id: animation-frames-count-video-frames
kind: rule
title: An animation's frames are whole video frames from 1 to 255, so at 30 Hz an art shown for one frame is never seen
codes: []
fixture: twinstar
---
`sprites.animation(arts, frames)` takes each art's time as a whole number of video frames, 1 to 255, at 60
a second, whatever the toy's update rate. A toy that updates at 30 Hz advances the clock 2 at a time, so an
art held for one frame can be skipped and odd counts show unevenly. Give every art at least 2 frames there,
and even counts where you can.

```luau
local _ships = dma("ships", {palette = "ships_pal", char = 0x4000, pal = 0})
local bits = dma("bits", {palette = "bits_pal", char = 0x4800, pal = 6})
local A = sprites.art({{size = 8, columns = 1, cells = {3}}}, bits)
local B = sprites.art({{size = 8, columns = 1, cells = {2}}}, bits)
local SPIN = sprites.animation({A, B}, {6, 6}) -- 6 video frames each: a tenth of a second
local fx = sprites.group()

function init()
  mode = 1
  screen.main.obj = true
  obj.char_base = _ships.char
end

function frame(t, f)
  fx:draw(SPIN, 120, 100, {key = 1})
end
```
