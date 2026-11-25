---
id: drum-dynamics-are-velocity
kind: recipe
title: Drum dynamics are vel, with hats under the snare and a snare roll that climbs into the next section
codes: []
---
A drum is a one-shot that plays to its end at its own pitch, so its only expression is how loud each
hit is: a note's `vel` in a song. Hats sit at about a third of the snare's `vel`, with the
on-beat hats a little over the off-beat ones so the pulse leans. A snare roll is four to eight short
hits with `vel` climbing into the first beat of the next section, where a `crash` lands.

Out of a song, the same roll is presets of climbing `vol` keyed by [[sfx]] on one effect voice
([[voices-for-songs-and-effects]]), each bound once at file level to its own local over the one `snare`
sample, and each played with a constant voice and note, so a cartridge can hold it ([[snes.sound]]). `"C3"`
is the drum's own pitch: the built-in drums play an octave under C4.

```luau
local soft = bank("snare", {vol = 30})
local mid = bank("snare", {vol = 50})
local loud = bank("snare", {vol = 75})
local full = bank("snare", {vol = 105})

function init()
  dsp.mvol = {l = 100, r = 100}
end

function frame(t, f)
  local step = f % 60
  if step == 0 then
    sfx(soft, 6, "C3")
  elseif step == 6 then
    sfx(mid, 6, "C3")
  elseif step == 12 then
    sfx(loud, 6, "C3")
  elseif step == 18 then
    sfx(full, 6, "C3")
  end
end
```
