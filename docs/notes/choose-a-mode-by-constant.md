---
id: choose-a-mode-by-constant
kind: rule
title: Choose between behaviours with a constant at the top of the file, so a cartridge carries only one
codes: []
fixture: adventure
---
When a game offers two ways to do one thing (a free [[camera-dead-zone|dead-zone camera]] or a
[[camera-room-by-room|room-by-room]] one), pick with a `local MODE = "follow"` constant at the top of
the file and branch on it. A constant the code never changes is known when the cartridge is built,
so it keeps only the branch the constant picks; switching is an edit, not a menu. A value set at run
time (from the pad or a save) keeps both paths and pays for the test every frame.

Another way is one more branch beside these.

```luau
local world = require("@ppu/world")
local _tiles = dma("tiles", {palette = "tiles_pal", char = 0x1000, pal = 32})
local w = world.load("overworld")
local MODE = "follow" -- or "rooms": edit to switch
local camx = makeBounded(0, 0, 1792) -- MAP SIZE
local x = makeBounded(128, 0, 2047)

function init()
  mode = 1
  w:bind("ground", bg[2], {map = 0x0000})
  w:follow(0, 0)
  screen.main.bg2 = true
end

function frame(t, f)
  if pad.right then x = math.min(x + 1, 2047) end
  if MODE == "rooms" then
    camx = math.min(w.max_x, x // 256 * 256)
  else
    camx = math.clamp(x - 128, 0, w.max_x)
  end
  w:follow(camx, 0)
end
```
