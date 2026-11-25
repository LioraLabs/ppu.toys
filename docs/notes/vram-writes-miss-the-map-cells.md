---
id: vram-writes-miss-the-map-cells
kind: rule
title: A vram[] write beside bg[n].map cells, @ppu/hud text among them, must provably miss the cells
codes: [snes.map]
---
A `vram[]` write in the same program, `@ppu/hud` text among them, must provably miss the cells' words: a
32-wide layer holds `map_base` to `map_base + 1023`, a two-screen one twice that. Give the HUD a descriptor
whose `map` is a number written in the table, on a base of its own, and show it on another layer; a write
whose address the build cannot bound, or one that can reach the cells, is refused at your line.

```luau
local hud = require("@ppu/hud")
local HUD = {map = 0x6800, width = 32, height = 4, palette = 7}

function init()
  mode = 1
  bg[1].map_base = 0x0000
  bg[1].screen_size = 0
  bg[3].map_base = 0x6800
  screen.main.bg1, screen.main.bg3 = true, true
  bg[1].map[3] = {}
  bg[1].map[3][5] = {tile = 1}
  hud.text(HUD, 1, 1, "SCORE")
end

function frame(t, f) end
```
