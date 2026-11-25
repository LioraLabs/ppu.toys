---
id: each-scene-starts-its-song
kind: rule
title: A scene switch restarts the sound chip, so each scene starts its own song and sounds in init
codes: []
fixture: twinstar
---
[[scenes|scenes.go]] enters the next scene with a cold boot, and the sound chip restarts with it: the
song that was playing stops, and an effect keyed just before the switch is cut. So each scene's `init`
starts the song it wants, and a sound that marks entering a scene (a start chime) plays there, not in
the frame that calls `scenes.go`. `sound.songs` names the songs from `score{..., at = false}` handles at
file level; `music:play` stops whichever score is playing, from this set or any other (one plays at a time).
Declared in the scene's own file, as here, the songs upload with that scene and count only in its sound RAM
([[snes.audio_budget]]); declare a song every scene plays (a jingle) in main.luau or a module it requires, and play
it over the scene's theme with no stop first: the jingle stops the theme, and the scene plays its theme again after.

```luau
local sound = require("@ppu/sound")
local music = sound.songs({
  title = score{song = "twinstar-title", at = false, loop = true},
  battle = score{song = "twinstar-battle", at = false, loop = true},
})

function init()
  sound.volume(100)
  music:play("title")
end

function frame(t, f)
  if pad.pressed.start then music:play("battle") end
end
```
