---
id: song-pitch-tops-out-at-c6
kind: gotcha
title: A note above C6 plays as C6, because the pitch register stops at four times a sample's recorded rate
codes: []
---
A built-in melodic sample is recorded at C4 and the chip's 14-bit pitch stops at `0x3fff`, a hair under
four times that: `note("C7")` is clamped and sounds as C6. Write a lead's top notes at C6 or below; a
part that must sound higher needs a sample recorded higher, placed with `base` set to its note.
The same limit applies to song rows. Drums are one-shots recorded at 16 kHz, which [[bank]] plays at
pitch `0x0800` for you, so a drum's note is a tuning, not a pitch to keep under C6.

```luau
local sound = require("@ppu/sound")
local lead = bank("lead", {vol = 70})

function init()
  sound.volume(100)
end

function frame(t, f)
  if pad.pressed.a then sfx(lead, 5, "C6") end -- the highest note there is
  if pad.pressed.b then sfx(lead, 5, "C5") end
end
```
