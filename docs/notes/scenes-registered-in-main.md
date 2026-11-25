---
id: scenes-registered-in-main
kind: recipe
title: A game is a list of scenes registered once in main.luau under UPPERCASE globals, and the first added starts
codes: []
---
Each part of a game (title, play, game over) is a file that returns its own `init` and `frame`.
main.luau registers every one with [[scenes.add|scenes.add(require("./title"))]], written inline, all
in that one file: a cartridge refuses a `scenes.add` anywhere else. The program starts in the first
scene added. [[scenes.go|scenes.go(PLAY)]] switches on the next update with a
[[scene-init-calls-setup|cold boot]].

Keep each handle in an UPPERCASE global (`TITLE`, `PLAY`), not a local: every scene file can then name
the next one without requiring main.luau. A module main.luau requires belongs to the whole program
(see [[placements-a-scene-owns|what a scene owns]]).

```luau
-- main.luau
TITLE = scenes.add(require("./title"))
PLAY = scenes.add(require("./play"))

-- title.luau
local function init() end

local function frame(_t: number, _f: number)
  if pad.pressed.start then scenes.go(PLAY) end
end

return {init = init, frame = frame}

-- play.luau
local function init() end

local function frame(_t: number, _f: number)
  if pad.pressed.select then scenes.go(TITLE) end
end

return {init = init, frame = frame}
```
