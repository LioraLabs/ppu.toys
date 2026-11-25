---
id: snes.note
kind: diagnostic
title: A note is a name such as "C4", "C#4" or "Db4", or a MIDI number
codes: [snes.note]
---
`error[snes.note]: note: bad note 'x'` means a pitch given to [[note]] or [[sfx]] is neither a note
name nor a number. A name is a letter A to G, an optional `#` or `b`, and an octave, which may be
negative: `"C4"`, `"C#4"`, `"Db4"`, `"A-1"`. A number is a MIDI note, 60 being C4. The same code
names a song source row whose note is bad: `score: row N bad note 'x'`.

`note(n, base)` is relative to the note the sample was recorded at (`"C4"` unless the preset says
otherwise), so a sample sounds at its own pitch at its base note.

```luau
local chime = bank("bell", {vol = 80})

function frame(t, f)
  if pad.a then sfx(chime, 7, "E5") end
  if pad.b then sfx(chime, 7, 69) end
end
```
