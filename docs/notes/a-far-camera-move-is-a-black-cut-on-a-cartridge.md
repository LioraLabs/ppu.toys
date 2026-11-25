---
id: a-far-camera-move-is-a-black-cut-on-a-cartridge
kind: gotcha
title: A follow draws every row and column the move uncovers, but a cartridge turns a move past its VBlank budget into a black cut frame
codes: []
fixture: platformer
---
The camera may move as far as the game likes in a frame, and nothing is refused: a follow writes every row and column the move uncovers (8 px is one row and one column, 40 px five of each, a jump across the map the
whole window). On a cartridge those cells are copied in the next vertical blank, and only about 1 KB of map fits there beside the sprites, and at most 16 strips (a row or a column each) a follow. A move
that uncovers more becomes a cut: the screen goes black for a picture, the whole window redraws, and then it shows, with no torn cells. The preview draws such a move at once, so a long move can show a black frame on a cartridge that the Studio never shows. Keep each step small ([[camera-glide]]), and ask for the cut on purpose for a teleport with `w:follow(x, y, {cut = true})`.

```luau
local world = require("@ppu/world")
local _tiles = dma("tiles", {palette = "tiles_pal", char = 0x1000, pal = 32})
local w = world.load("meadow")
local x = makeBounded(640, 0, 1280)

function init()
  mode = 1
  w:bind("ground", bg[2], {map = 0x0000})
  w:follow(640, 0, {cut = true}) -- a teleport, asked for on purpose: the whole window, once
  screen.main.bg2 = true
end

function frame(t, f)
  x = math.min(1280, x + 2) -- small steps: a few cells a frame
  w:follow(x, 0)
end
```
