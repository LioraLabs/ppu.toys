---
id: camera-room-by-room
kind: recipe
title: A room-by-room camera holds on one screen and slides to the next when the hero walks off its edge
codes: []
fixture: adventure
---
Cut the map into screen-sized rooms (256 by 224). The camera holds on the room holding the hero; when
his feet cross into another room, it slides there a few pixels a frame (4 is about a second), and the
game freezes him while it slides. When the slide ends, walk him a few frames into the new room on his
own so he does not stand on the edge he crossed, and wake that room's enemies.

`room_of` is a division and a multiply, clamped to the map ([[clamp-the-camera]]). The slide's steps
stay small, so the world streams a column or a row a frame ([[follow-once-a-frame]]).

```luau
local world = require("@ppu/world")
local _tiles = dma("tiles", {palette = "tiles_pal", char = 0x1000, pal = 32})
local w = world.load("overworld")
local SLIDE = 4
local camx, camy = makeBounded(0, 0, 1792), makeBounded(0, 0, 1792) -- MAP SIZE
local roomx, roomy = makeBounded(0, 0, 1792), makeBounded(0, 0, 1792)
local x, y = makeBounded(128, 0, 2047), makeBounded(112, 0, 2015)

local function room_of(px: number, py: number): (number, number)
  return math.min(w.max_x, px // 256 * 256), math.min(w.max_y, py // 224 * 224)
end

function init()
  mode = 1
  w:bind("ground", bg[2], {map = 0x0000})
  w:follow(0, 0)
  screen.main.bg2 = true
end

function frame(t, f)
  local sliding = camx ~= roomx or camy ~= roomy
  if not sliding then
    if pad.right then x = math.min(x + 1, 2047) end
    roomx, roomy = room_of(x, y)
  end
  camx += math.clamp(roomx - camx, -SLIDE, SLIDE)
  camy += math.clamp(roomy - camy, -SLIDE, SLIDE)
  w:follow(camx, camy)
end
```
