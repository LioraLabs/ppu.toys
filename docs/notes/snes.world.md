---
id: snes.world
kind: diagnostic
title: A cartridge streams a world from fixed shapes, loaded at file level and followed once an update
codes: [snes.world]
fixture: platformer
---
A cartridge builds each [[@ppu/world|Tiled world]] into ROM and plans its streaming ahead of time, so
`@ppu/world` has to be used in shapes it can read. `error[snes.world]` names the one that broke (the
preview gives the same code for the mistakes it catches too):

- `local world = require("@ppu/world")` and `local w = world.load("level1")` at file level, the
  map's name and any options written as literals.
- `w:bind(...)` directly in `init()`, not under an `if`, a loop or a helper.
- `w:follow(x, y)` once an update: in `init()`, in `frame()`, or in one helper `frame()` calls once.
- `w:find("start")` with a literal name. A world has `bind`, `follow`, `tile`, `set`, `find` and
  `enter`; `w:enter` needs every map it can enter loaded above it in the same file.
- A world shared by scenes is loaded in a module that returns it in a table, `return {w = w}`, and
  that `main.luau` requires, so it is the program's; each file names it at file level,
  `local w = Track.w`. A module only one scene requires belongs to that scene.
- A helper can take the world as an argument (`local function show(w, x, y)`): `tile`, `find`,
  `set` and `follow` work on it when every call passes the same world. Its follow counts where the
  helper is called, so call it once an update as a statement in `frame()`; `bind` and `enter`
  stay in the file that names the world.
- The map itself: tile classes and object property names that are Luau names, whole-pixel object
  positions, and property values that are booleans, numbers or strings.

```luau
local world = require("@ppu/world")
local _tiles = dma("tiles", {palette = "tiles_pal", char = 0x1000, pal = 32})
local w = world.load("meadow")
local cam = makeBounded(0, 0, 1280)

function init()
  mode = 1
  bg[2].char_base = 0x1000
  w:bind("ground", bg[2], {map = 0x0000})
  w:follow(cam, 0)
  screen.main.bg2 = true
end

function frame(t, f)
  if pad.right then cam = math.min(cam + 2, w.max_x) end
  if pad.left then cam = math.max(cam - 2, 0) end
  w:follow(cam, 0)
end
```
