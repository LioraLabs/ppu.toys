---
id: snes.audio_budget
kind: diagnostic
title: Sound must fit the sound CPU's 64 KiB, at most 256 different sound effects, and a sound effect's len must fit the count the cartridge keeps
codes: [snes.audio_budget]
---
`error[snes.audio_budget]` has four causes. The first: the songs, samples and echo buffer do not fit the sound CPU's
64 KiB of sound RAM. The sound driver takes the first 4,096 bytes; the program's songs come next (their notes, the
sound effects' packets, 12 bytes each, and a directory of every song), then the entering scene's own upload (the
program's samples, the scene's own samples, the scene's own songs), and the echo buffer at the top, 2,048 bytes per
step of the longest `dsp.echo.delay` the program writes ([[echo-is-the-snes-texture]]). They share whatever the echo
leaves, and each part is sent in one upload of at most 32,640 bytes (the program's songs' upload also carries the
driver, so they hold 29,056). Each [[scenes|scene]] entry uploads its own part again, so the limit is the largest
scene's: a song or sample declared in a scene's file, or a module only that scene requires, is that scene's; one
declared in main.luau, a module main.luau requires, or a module two scenes require is the program's and counts in
every scene. The message names the scene that overflows and lists each song's and each sample's bytes, biggest first,
the total and the room left for them. Use fewer or shorter samples, loop shorter patterns, a shorter echo delay, or
move a song or sample only one scene plays into that scene's file.

The second: an [[sfx|sfx()]] holds its voice longer before the key-off than the cartridge can count. The longest is
15,728 frames (262.13 s) at any update rate, and the program is refused past it. Use a shorter `len`, or `len = false`
to hold the note until the voice plays again (a cartridge has no `koff()`: [[cart-has-no-key-on-or-off]]); an
automatic effect with `len = false` keeps its voice, and the song loses it, until another effect takes it. The third: the program plays more than 256 different sound effects (sound, voice, note and len together), and the
cartridge's sound driver numbers 256. Play fewer notes or named voices per sound. At most 256 different samples fit
the sample directory.

The fourth: a scene's song plays from code another scene, or the program, owns. Only the owning scene's upload holds
it, so the message names both scenes: play it from its own scene, or declare its `score{}` in main.luau (or a module
main.luau requires) so that every scene holds it.

```luau
local zap = bank("pluck")
local hum = bank("strings")

function frame(t, f)
  if pad.a then sfx(zap, 0, "C5", {len = 0.4}) end
  if pad.b then sfx(hum, 1, "C3", {len = false}) end
end
```
