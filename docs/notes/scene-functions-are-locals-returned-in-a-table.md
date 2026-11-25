---
id: scene-functions-are-locals-returned-in-a-table
kind: rule
title: A scene module keeps init and frame local and returns them in a table, because the engine calls them through the table
codes: []
fixture: twinstar
---
A scene is a module that returns `{init = init, frame = frame}` (and optionally `preview`, a local function of the file that only Scene view runs, so nothing else may name it: put what the preview and `init` share in another local function). Declare both functions `local`: the engine calls them through the
returned table, never as globals, and a program of scenes has no global `frame()` or `init()` of its own. `frame` is required and `init` is optional, so a scene with nothing to set up returns only `frame`. The scene's file-level code places its graphics, as any program does; its `init`
sets registers (see [[scene-init-calls-setup]]), because a [[scenes-registered-in-main|switch]] cold-boots them. `t` and `f` count from 0 in each scene.

```luau
-- main.luau
scenes.add(require("./title"))

-- title.luau
local sheet = dma("ships", {palette = "ships_pal", char = 0x4000, pal = 0})
local SHIP = sprites.art({{size = 16, columns = 1, cells = {0}}}, sheet)
local ships = sprites.group()

local function init()
  mode = 1
  screen.main.obj = true
  obj.char_base = sheet.char
end

local function frame(t, f)
  ships:draw(SHIP, 128 + math.sin(t) * 40, 104)
end

return {init = init, frame = frame}
```
