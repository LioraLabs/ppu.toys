---
id: cart-mixer-is-master-volume
kind: gotcha
title: A cartridge's mixer is the master volume and the echo; the other chip registers are refused
codes: [snes.unsupported]
---
Of the sound chip, a cartridge takes the master volume ([[dsp.mvol]], both channels 0 to 127, written
whole or one channel at a time), the echo as constants set once in a scene's setup (`dsp.echo`,
`dsp.evol` and `voice[n].echo`: [[echo-is-the-snes-texture]]), a voice's `vol.l`, `vol.r` and `pitch`,
and the read-only `voice[n].envx` ([[voice-envelope-drives-visuals]]). Noise, pitch modulation and
direct envelope or sample pokes have no cartridge form, and neither has a `bank()` `echo` option: a
cartridge echoes voices, not sounds ([[snes.sound]]).
Shape a sound with its [[bank]] preset (`vol`, `pan`, `adsr`, `len`), play it with [[sfx]] or a
[[score]], and set the volume once in each scene's setup ([[sound-chip-powers-on-silent]]).

```luau
function init()
  dsp.mvol = {l = 96, r = 96}
end

function frame(t, f)
end
```
