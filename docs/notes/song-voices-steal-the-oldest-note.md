---
id: song-voices-steal-the-oldest-note
kind: concept
title: When a song needs more voices than it has, the note that started earliest is cut
codes: []
fixture: twinstar
---
A song plays on its own voices ([[voices-for-songs-and-effects]]). Each note takes the lowest free one; when
none is free, the note that started earliest is cut short and its voice goes to the new note. So a long held
pad under fast chords is the first to vanish, in the Studio and on a cartridge alike. Keep chords within the
song's voices and shorten held notes that overlap the next ones; `song_check` names the places where more
notes want to sound than the song has voices.

```luau
local battle = score{song = "twinstar-battle", loop = true}

function frame(t, f)
end
```
