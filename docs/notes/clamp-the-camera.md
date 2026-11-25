---
id: clamp-the-camera
kind: rule
title: Clamp the camera to 0..w.max_x and 0..w.max_y, and bound it with the map's size written as a literal
codes: []
fixture: platformer
---
`w:follow(x, y)` takes the camera's top left in map pixels, x from 0 to `w.max_x` and y from 0 to
`w.max_y` (the map's size less one screen, 256 by 224; 0 for a map no bigger than the screen). Work
out where the camera wants to be, then clamp it there every frame: `math.clamp(x - 128, 0, w.max_x)`
keeps the hero centred and stops at the map's ends.

The camera lasts between frames, so it is a [[makeBounded]] value, and its bounds are literal numbers:
write the map's `max_x` itself (1280 for a 1536 px wide map) and mark it, so you change it when the
map grows in Tiled. A cartridge stops on a value outside the bound ([[snes.halt]]). Every camera
recipe ends here: [[camera-dead-zone]], [[camera-room-by-room]], [[camera-vertical-band]],
[[camera-auto-scroll]], [[camera-glide]].

```luau
local world = require("@ppu/world")
local _storm = dma("storm", {char = 0x1000, pal = 0})
local w = world.load("castle")
-- MAP SIZE: castle.tmj is 1024x672 px, so the camera's range is 768x448. Change with the map.
local camx, camy = makeBounded(0, 0, 768), makeBounded(0, 0, 448)
local x, y = makeBounded(40, 0, 1023), makeBounded(600, 0, 671)

function init()
  mode = 1
  w:bind("ground", bg[2], {map = 0x0000})
  w:follow(0, w.max_y)
  screen.main.bg2 = true
end

function frame(t, f)
  camx = math.clamp(x - 128, 0, w.max_x)
  camy = math.clamp(y - 112, 0, w.max_y)
  w:follow(camx, camy)
end
```
