---
id: tiled-map-names-its-source
kind: rule
title: A Tiled map is a project file, and its tileset image's path is what binds it to a sheet source
codes: []
fixture: platformer
---
A level drawn in Tiled is data, not art: the map is a `.tmj` (JSON) and its tileset an external
`.tsj`, both listed in `ppu.json`'s `files` beside the code. The tileset's `image` path names the
source it draws from: `"image": "assets/tiles.png"` draws from the source `tiles`, a `sheet` (or `m7`)
placed with [[dma]] as usual. Tiles are 8x8 or 16x16, with no margin or spacing.

Tile layers are CSV or plain Base64 in Tiled; written by hand, a layer's `data` is a JSON array of
tile numbers. A `.tmx` map, compressed layers and rotated tiles (the SNES only flips) are refused.
An empty cell draws the sheet's first fully transparent tile, so keep a blank tile in the image.

```luau
local world = require("@ppu/world")
-- meadow.tmj's tileset tiles.tsj names assets/tiles.png: the `tiles` source.
local tiles = dma("tiles", {palette = "tiles_pal", char = 0x1000, pal = 32})
local w = world.load("meadow")

function init()
  mode = 1
  bg[2].char_base = tiles.char
  w:bind("ground", bg[2], {map = 0x0000})
  w:follow(0, 0)
  screen.main.bg2 = true
end
```
