---
id: envelopes-belong-to-sounds
kind: recipe
title: An envelope belongs to a sound, so a song picks struck or swelling by its sound, and bank() shapes an effect's
codes: []
fixture: twinstar
---
A song row is a sound, a pitch, a `vol` and a pan: it carries no envelope. It plays with its sound's
built-in preset envelope, and every row naming that sound shares it, so choose the sound for its shape.
`a` runs 0 to 15 (15 is instant, 9 swells in over a short moment), `d` 0 to 7 is how fast it falls to the
sustain level `s` (0 to 7), and `r` 0 to 31 is how fast it fades from there.

- **Struck and decaying**, for comping stabs ([[chords-as-arpeggios-or-stabs]]): `piano`, `pluck` or
  `bell`, an instant attack falling to a low sustain, re-struck on the beat.
- **Swelling**, for pads: `strings`, a slower attack and a full sustain, so a held chord grows in.
- **Held**, for a lead or an organ line: `lead`, `organ` or `flute`, full sustain until the note ends.

For an effect, `bank("piano", {adsr = ...})` at file level makes another preset over the same placed
sample, with its own constant envelope, played by [[sfx]] ([[snes.sound]]). Editing `BANK.piano.adsr`
reshapes a song's sound in the preview only: a cartridge refuses it, so the song would sound different
built.

```luau
local stab = bank("piano", {adsr = {a = 15, d = 6, s = 2, r = 14}})
local swell = bank("strings", {adsr = {a = 5, d = 2, s = 6, r = 4}})
local battle = score{song = "twinstar-battle", loop = true}

function frame(t, f)
  if pad.pressed.a then
    sfx(stab, 6, "C5")
  end
  if pad.pressed.b then
    sfx(swell, 7, "G4")
  end
end
```
