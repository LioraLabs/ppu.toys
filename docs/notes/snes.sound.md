---
id: snes.sound
kind: diagnostic
title: A cartridge sound is a bank() at file level, played by name with sfx; voice and dsp registers are written, never read
codes: [snes.sound]
---
`error[snes.sound]: ...` names a sound a cartridge cannot hold. A cartridge keeps every [[bank]]
sound in ROM and plays it through [[sfx]], so the rules are tighter than the preview's:

- `bank option "echo" has no cartridge lowering; only vol, pan, len and constant adsr do`: a
  cartridge echoes voices, not sounds, and has no noise or pitch modulation. Delete the option, shape
  the sound with `vol` (0 to 127), `pan` (-1, 0 or 1), `len` and a constant `adsr`, and set
  `voice[n].echo = true` in `init()` for the voices that play it ([[echo-is-the-snes-texture]]).
- `dsp.noise_clock has no cartridge form` (or `dsp.mute`): delete the line. A cartridge writes
  `dsp.mvol`, `dsp.evol` and `dsp.echo`; set them once in a scene's setup.
- `dsp.echo.delay is not a constant here` (or another echo setting, or `voice[n].echo`): the echo is
  set up as a scene starts and its buffer sized for the longest delay, so every value is a constant,
  written once in `init()`: `dsp.echo = {delay = 4, feedback = 64, fir = {127, 0, 0, 0, 0, 0, 0, 0}}`,
  `dsp.evol = {l = 40, r = 40}`, `voice[2].echo = true`. Two settings mean two scenes, or two
  constants the code picks between.
- `sfx needs a sound the cart can name`: the first argument of `sfx` is not a local bound to
  `bank()`. It usually follows a refused `bank()` line above it: fix that one and this goes too.
  Otherwise bind the sound once, `local zap = bank("pluck")`, and pass `zap` itself.
- `Call to "bank" (...) has no cartridge lowering here`: `bank()` ran inside a function. Call it at
  file level, or in a record of a constant table.
- `note() has no cartridge form`: `note("B4")` ran inside a function. Pass the name to `sfx`
  (`sfx(zap, 1, "B4")`), or keep the pitch as a constant: `0x1000` plays the sample at its recorded
  note, and each semitone up multiplies it by 2^(1/12), so B4 from a sample recorded at C4 is
  `0x1E34`.
- `Voice pan and pitch controls are write-only on the cartridge` (or an echo setting `is write-only`):
  the code reads `voice[n].pitch`, `voice[n].vol` or the echo. Keep the value in your own bounded state, change that, and write it to the
  voice; `voice[n].envx` is the voice level a cartridge does let you read.

A pitch sweep kept in a variable:

```luau
local zap = bank("bell", {vol = 104})
local pitch = makeBounded(0x1000, 0x400, 0x3FFF)

function init()
  dsp.mvol = {l = 112, r = 112}
end

function frame(t, f)
  if pad.pressed.a then
    sfx(zap, 1, "B5")
    pitch = 0x1000
  end
  pitch = math.min(0x3FFF, pitch + 0x190)
  voice[1].pitch = pitch -- not voice[1].pitch + 0x190
end
```
