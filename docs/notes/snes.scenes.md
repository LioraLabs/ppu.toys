---
id: snes.scenes
kind: diagnostic
title: A cartridge reads scenes from fixed shapes, registered in main.luau and switched by a statement
codes: [snes.scenes]
fixture: title
---
A cartridge plans each scene when it builds, so [[scenes]] has to be written in shapes it can read
ahead of time. `error[snes.scenes]` names the one that broke:

- `scenes.add(require("./title"))`, with the require inline, and every `scenes.add` in main.luau.
- `scenes.go(GAME)` as a statement of its own, with one scene handle; a fade is
  `scenes.go(GAME, {fade = 15})`, a literal 1 to 255 frames, and only from a scene's `frame()`.
- In a scene file, each `dma()` and `score{}` is a file-level statement of its own, bound to a name
  (`local tune = score{song = "theme"}`).

The handles are globals, so each scene can name the next one; the program starts in the first
scene added.

```luau
-- main.luau
local _Shared = require("./shared")

TITLE = scenes.add(require("./title"))
PLAY = scenes.add(require("./play"))

-- shared.luau
return {}

-- title.luau
local function frame(t, f)
  if pad.pressed.start then scenes.go(PLAY, {fade = 15}) end
end
return {frame = frame}

-- play.luau
local function frame(t, f)
  if pad.pressed.start then scenes.go(TITLE) end
end
return {frame = frame}
```
