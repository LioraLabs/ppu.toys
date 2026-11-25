---
id: a-mode-7-source-holds-256-distinct-tiles
kind: gotcha
title: A Tiled map on a Mode 7 tileset image with more than 256 distinct 8x8 tiles is refused with its count, so reuse tiles in the image until it has 256 or fewer
codes: []
fixture: racer
---
The plane has one set of 256 tiles, and a Mode 7 image is cut into 8x8 tiles with exact duplicates shared. A tileset image that needs more is refused when a Tiled map uses it, naming how many distinct tiles it has (without a map, the extra tiles fall back to tile 0). Redraw the image
with repeated tiles (grass, road and kerb pieces reused), as a game map would. The map sits at the plane's top left and a map up to 128x128 cells loads whole, with every empty cell drawing
the source's first fully transparent tile, so keep a blank tile in the image ([[tiled-map-names-its-source]]). A larger map streams ([[mode-7-track-streams-within-radius]]).

```luau
-- tiles.png is the Mode 7 tileset's image; its distinct 8x8 tiles must number 256 or fewer.
local _circuit = dma("tiles", {palette = "tiles_pal"})

function init()
  mode = 7
  m7.a, m7.d = 1, 1
  screen.main.bg1 = true
end

function frame(t, f)
  m7.cx, m7.cy = 128, 128
end
```
