---
id: parallax-second-background
kind: recipe
title: Parallax is a second background scrolled at a fraction of the camera
codes: []
fixture: platformer
---
Put a sky or a far range on another background, placed once with [[place-art-with-dma|dma()]], and
each frame scroll it by a fraction of the camera: `bg[3].scroll.x = cam // 4` moves it at a quarter
of the map's speed, so it reads as far away. Integer division keeps the scroll whole pixels. A
vertical level does the same with `scroll.y` and a smaller fraction. In Mode 1, bg3 draws 2bpp tiles,
so its colours sit in the first palettes.

When the far layer is itself a Tiled layer of the same map, bind it with
`w:bind("far", bg[3], {parallax = 0.25, wrap = true})` instead: one follow streams it at that rate.
The camera comes from [[clamp-the-camera]].

```luau
local world = require("@ppu/world")
local _tiles = dma("tiles", {palette = "tiles_pal", char = 0x1000, pal = 32})
local _sky = dma("sky", {palette = "sky_pal", char = 0x3000, map = 0x0800, pal = 0})
local w = world.load("meadow")
local cam = makeBounded(0, 0, 1280)

function init()
  mode = 1
  bg[3].char_base, bg[3].map_base = 0x3000, 0x0800
  w:bind("ground", bg[2], {map = 0x0000})
  w:follow(0, 0)
  screen.main.bg2, screen.main.bg3 = true, true
end

function frame(t, f)
  cam = math.clamp(f % 1536 - 128, 0, w.max_x)
  w:follow(cam, 0)
  bg[3].scroll.x = cam // 4 -- a quarter of the camera's speed
end
```
