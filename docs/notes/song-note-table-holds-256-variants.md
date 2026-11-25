---
id: song-note-table-holds-256-variants
kind: gotcha
title: A cartridge song holds at most 256 different notes, counting pitch, sound, volume and pan together
codes: []
fixture: twinstar
---
A song source's notes are stored once each and replayed by number, so the build counts the distinct
combinations of pitch, sound, volume and pan: past 256 it stops, naming the song, and asks for fewer
velocities or pans. A part that plays every note at its own velocity, or pans each hit differently,
spends the table fastest; keep a handful of velocities (accent and normal), not one per note.

The fix is in the song source, not in the code that plays it:

```luau
local music = score{song = "twinstar-battle", loop = true}

function frame(t, f)
end
```
