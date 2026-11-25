---
id: pause-by-muting-master-volume
kind: recipe
title: Pause mutes the master volume, and the song keeps time underneath
codes: []
fixture: twinstar
---
A paused game stops its own updates, but the song keeps its clock. Set the master volume to 0 while
paused (`sound.volume` or [[dsp.mvol]]) and back when play resumes: the music comes back in time, at
the bar it would have reached, with no restart to manage.

```luau
local sound = require("@ppu/sound")
local music = sound.songs({battle = score{song = "twinstar-battle", at = false, loop = true}})
local paused = false

function init()
  sound.volume(100)
  music:play("battle")
end

function frame(t, f)
  if pad.pressed.start then
    paused = not paused
    sound.volume(if paused then 0 else 100)
  end
  if paused then return end
end
```
