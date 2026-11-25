---
id: camera-vertical-band
kind: recipe
title: A camera that moves vertically only when the feet leave a band keeps a jump from shaking the screen
codes: []
fixture: platformer
---
In a level that scrolls both ways, centring the camera on the hero's y makes every jump bob the
screen. Keep a band instead (96 to 160 pixels down the screen in the platformer's castle): the camera
moves up or down only when the feet leave it, by just enough to bring them back. Across, it still
centres on him. Clamp both to the map ([[clamp-the-camera]]).

The band is one clamp of the camera between the two positions that keep the feet at its edges.
Start the camera at the bottom of the map when the level starts there, then let the band settle it.

```luau
local world = require("@ppu/world")
local _tiles = dma("storm", {palette = "storm_pal", char = 0x1000, pal = 32})
local w = world.load("castle")
local camx, camy = makeBounded(0, 0, 768), makeBounded(448, 0, 448) -- MAP SIZE
local x, y = makeBounded(40, 0, 1023), makeBounded(600, 0, 671)

local function aim()
  camx = math.clamp(x - 128, 0, w.max_x)
  camy = math.clamp(math.clamp(camy, y - 160, y - 96), 0, w.max_y)
end

function init()
  mode = 1
  w:bind("ground", bg[2], {map = 0x0000})
  aim()
  w:follow(camx, camy)
  screen.main.bg2 = true
end

function frame(t, f)
  if pad.up then y = math.max(y - 2, 0) end
  aim()
  w:follow(camx, camy)
end
```
