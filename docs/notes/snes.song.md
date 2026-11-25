---
id: snes.song
kind: diagnostic
title: song{} is gone, so music is a song source played with score{}
codes: [snes.song]
fixture: twinstar
---
`error[snes.song]: song{} is gone` means the code builds music from a `song{}` table, which neither
the preview nor a cartridge runs any more. Music is a song source: a `.song.json` in the folder
([[a-song-json-file-is-a-song-source]]), a `.mid` ([[a-mid-file-is-a-song-source]]) or a song made in
the Studio's sequencer, listed in ppu.json as a `song` source and played by name with
[[snes.score|score{}]]. A `score{}` in a scene's own file uploads with that scene and plays only there; one in
main.luau, or a module it requires, is in every scene ([[snes.audio_budget]]). One score plays at a time: any
score's `play()` stops the one playing ([[one-score-starts-by-itself]]).

```luau
local title = score{song = "twinstar-title", loop = true}

function frame(t, f)
end
```
