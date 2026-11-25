--!strict
-- A world drawn in Tiled: open project.tiled-project in Tiled, paint level1.tmj, save.
-- The tileset (tiles.tsj) is assets/tiles.png, the same image as the `tiles` source.
-- ppu docs worlds covers maps, collision and objects; ppu.d.luau declares the API for editors.
local world = require("@ppu/world")
local _tiles = dma("tiles", {char = 0x1000, pal = 0})
local w = world.load("level1")

-- The camera's left edge in map pixels: level1 is 64 cells wide, so 0 to 64 * 8 - 256.
-- The bound is fixed: resize the map in Tiled and change 256 here to its new w.max_x.
local x = makeBounded(0, 0, 256)

function init()
  mode = 1
  cgram[0] = rgb(96, 144, 248)
  bg[1].map_base = 0x0000
  w:bind("ground", bg[1])
  w:follow(x, 0)
  screen.main.bg1 = true
end

function frame(t: number, f: number)
  if pad.right then x = math.min(x + 2, w.max_x) end
  if pad.left then x = math.max(x - 2, 0) end
  w:follow(x, 0)
end
