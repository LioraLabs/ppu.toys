---
id: scene-init-calls-setup
kind: rule
title: A scene switch cold-boots the console, so every scene's init calls one shared setup()
codes: []
fixture: title
---
[[scenes.go]] enters the next scene with a cold boot: registers, VRAM, CGRAM and the sound chip go back
to power-on, and only module tables survive. A scene that relied on the last one's `mode`, layer
bases, `screen.main` or master volume shows black or plays silent.

Write the settings every scene shares once, in a `setup()` in shared.luau, and call it first in every
scene's `init`. Each scene then turns on its own layers and plays its own start sound after it. The
master volume is 0 at power-on: set it in `setup()` too, or nothing sounds.

```luau
-- main.luau
PLAY = scenes.add(require("./play"))

-- shared.luau
local sound = require("@ppu/sound")
local sheet = dma("sprites", {palette = "sprites_pal", char = 0x4000, pal = 0})
local font = dma("font", {palette = "font_pal", char = 0x5000, pal = 112})

local function setup()
  mode = 1
  bg[1].char_base, bg[1].map_base = font.char, 0x7c00
  screen.main.bg1, screen.main.obj = true, true
  obj.char_base = sheet.char
  sound.volume(100)
  cgram[0] = 0x2841
end

return {setup = setup}

-- play.luau
local Shared = require("./shared")

local function init()
  Shared.setup()
end

local function frame(_t: number, _f: number) end

return {init = init, frame = frame}
```
