---
id: snes.score
kind: diagnostic
title: score{} plays a song source named in ppu.json, once, at file level
codes: [snes.score]
fixture: twinstar
---
`error[snes.score]` means [[score]] could not start its song: no `song = "<name>"` was given, no
song source of that name is in the project, a sound a row names could not be placed in sound RAM,
or `at` was neither seconds nor `false`.

A song is a source of kind `song` listed in ppu.json: a `.song.json` or a `.mid` in the folder
([[a-song-json-file-is-a-song-source]], [[a-mid-file-is-a-song-source]]), or one made in the Studio's
sequencer.
`score{}` places its sounds, so it is setup work: call it at file level and keep the handle. `at =
false` sets it up silent until `play()`; `loop = false` plays it once through.

```luau
local battle = score{song = "twinstar-battle", at = false, loop = true}

function frame(t, f)
  if f == 60 then battle.play() end
end
```
