---
id: echo-is-the-snes-texture
kind: recipe
title: Echo is the SNES texture; switch the chip's echo on once in init(), for the voices of the lead
codes: []
---
The repeat trailing a melody is the sound of the era. Give it to the lead and harmony and keep bass and
drums dry: low notes repeated turn the mix to mud. The chip's echo is a delay with feedback and an 8-tap
filter, and a cartridge plays it as the preview does, set once in a scene's `init()`:

- `dsp.echo = {delay = ..., feedback = ..., fir = {...}}`: `delay` 1 to 15 in steps of 16 ms (so at most
  240 ms), `feedback` -128 to 127 (how much of the repeat repeats again), `fir` the filter's eight taps
  (`{127, 0, 0, 0, 0, 0, 0, 0}` passes the repeat unchanged). A field left out is 0.
- `dsp.evol = {l = ..., r = ...}`: how loud the repeats are, -128 to 127 a side; opposite signs spread them.
- `voice[n].echo = true` for each voice that feeds the echo. A song's kept parts take the lowest voices
  first, so `voice[0].echo = true` echoes a lead that is the song's only kept part, one note at a time.

On a cartridge every value is a constant: the sound driver sets the echo up as the scene starts, and the
cartridge keeps a buffer of 2,048 bytes of sound RAM per step of the longest `delay` the program writes, at
the top of sound RAM ([[snes.audio_budget]]). The chip's echo costs no voice and no song bytes.

Echo adds level: a mix that just fits clips once the repeat is in, so leave headroom in [[dsp.mvol]]. A
repeat that must change per section, or one longer than 240 ms (an eighth note below 125 beats a minute),
is written instead, as a second part ([[a-written-echo-trails-the-lead-by-an-eighth]]).

```luau
local lead = bank("flute", {vol = 100, pan = -1})

function init()
  dsp.mvol = {l = 96, r = 96}
  dsp.evol = {l = 36, r = -36}
  dsp.echo = {delay = 12, feedback = 48, fir = {127, 0, 0, 0, 0, 0, 0, 0}}
  voice[5].echo = true
end

function frame(t, f)
  if f % 60 == 0 then
    sfx(lead, 5, "E5")
  end
end
```
