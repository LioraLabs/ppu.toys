---
id: voices-for-songs-and-effects
kind: rule
title: The sound driver picks each effect's voice, borrowing one from the song until the effect ends
codes: []
---
The chip has eight [[voice|voices]], and a song may use all eight. `sfx(sound, note)` and a cue written
`{preset, note, priority}` leave the voice to the sound driver, in the preview and on a cartridge alike.
It takes, in order: a voice the song does not use; a voice the song rests on for longer than the effect
lasts; otherwise the highest voice the song is sounding. When every voice is already held by an effect, a
new effect cuts the oldest automatic one.

Taking a song's voice borrows it, it never kills the part: the effect sounds for its length (its `len`,
0.15 s for a looping sample without one, a one-shot's own length), the song's notes on that voice are
silent meanwhile, and the song's next note after it plays as written. An effect with `len = false` has no
end: it holds its voice until another effect takes it (a cartridge has no `koff()`), so the song loses that
voice; give an automatic effect a `len` instead.

Mark a part that must never drop out (a lead, a bass) `"keep": true` on its row
([[a-song-json-file-is-a-song-source]]). Kept parts take the lowest voices and no effect takes them, for
the whole song, even while the part rests, so the other parts share fewer voices. [[ppu-song-check|ppu song check]]
names the kept voices.

Name a voice, `sfx(sound, v, note)` or `{preset, v, note, priority}`, for effects that should cut each other
off (two hits on one voice). A named voice borrows the same way: the effect has it until its length ends,
the song's notes there silent meanwhile.

```luau
local sound = require("@ppu/sound")
local cues = sound.cues({
  shot = {bank("pluck", {vol = 36}), "C6", 1},
  pick = {bank("bell", {vol = 80}), "A4", 2},
  hit = {bank("tom", {vol = 96}), 6, "C3", 3}, -- hits on voice 6 cut each other off
})

function init()
  sound.volume(100)
end

function frame(t, f)
  if pad.pressed.a then cues:play("shot") end
  if pad.pressed.b then cues:play("hit") end
  if pad.pressed.x then cues:play("pick") end
end
```
