---
id: each-scene-sets-the-map-base-it-writes
kind: rule
title: In a game of scenes, each scene that writes a layer's cells sets that layer's map_base and screen_size in its own init()
codes: [snes.map]
---
A scene switch powers the console back on, so in a game of scenes each scene that writes a layer's cells
assigns its `map_base` and `screen_size` directly in its own `init()`, with the same numbers in every scene
that writes cells there. A scene that writes no cells on the layer may place a picture on it with `dma()` at
another base.

```luau
-- main.luau
TITLE = scenes.add(require("./title"))
PLAY = scenes.add(require("./play"))

-- title.luau
local function init()
  mode = 1
  bg[1].map_base = 0x7c00
  bg[1].screen_size = 0
  screen.main.bg1 = true
  bg[1].map[3] = {}
  bg[1].map[3][5] = {tile = 1}
end

local function frame(_t: number, _f: number)
  if pad.pressed.start then scenes.go(PLAY) end
end

return {init = init, frame = frame}

-- play.luau
local function init()
  mode = 1
  bg[1].map_base = 0x7c00
  bg[1].screen_size = 0
  screen.main.bg1 = true
  for x = 0, 31 do bg[1].map[x] = {} end
end

local function frame(_t: number, f: number)
  bg[1].map[f % 32][20] = {tile = 2}
  if pad.pressed.select then scenes.go(TITLE) end
end

return {init = init, frame = frame}
```
