---
id: w-tile-reads-cost-a-hundredth-of-a-frame
kind: concept
title: Each w:tile costs about a hundredth of a frame on a cartridge, so sample a few points per actor
codes: [snes.budget]
fixture: platformer
---
`w:tile(layer, x, y)` reads one map cell, and on a cartridge each call costs about 1% of a 60 Hz frame. Eight
actors sampling twelve points each spend a whole frame on it. Sample the points that decide something (the
feet, the head, the leading edge: two to four), share one read when two tests land in the same cell, and skip
actors off screen ([[only-update-actors-near-the-screen]]).

```luau
local world = require("@ppu/world")
local _tiles = dma("tiles", {palette = "tiles_pal", char = 0x1000, pal = 32})
local w = world.load("meadow")
local x = makeBounded(64, 0, 1000)
local y = makeBounded(64, 0, 400)

function init()
  mode = 1
  w:bind("ground", bg[2], {map = 0x0000})
  w:follow(0, 0)
  screen.main.bg2 = true
end

function frame(t, f)
  local feet = w:tile("ground", x + 8, y + 16) -- one read under the feet, not a grid
  if feet == 0 then y = math.min(400, y + 1) end
end
```
