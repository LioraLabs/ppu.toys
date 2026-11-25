---
name: making-toys
description: Make and change ppu.toys toys and games in this folder with the ppu CLI. Luau code drives an emulated SNES PPU and S-DSP (backgrounds, sprites, Mode 7, HDMA, windows, colour math, music), checked and rendered offline, written so the same code builds into a real SNES cartridge. Use for any edit to main.luau, ppu.json, the art or the music here.
---

# Making ppu.toys toys

This folder is a ppu.toys project: `ppu.json` lists the Luau files and the
sources (art, palettes, samples, songs). The `ppu` command runs it offline,
with the same engine as the ppu.toys Studio, and its docs ship inside it, at
the version that wrote this skill.

## Code first, then the docs

- Start by editing: the project's own files show the APIs they use. Then
  `ppu check .`, and read only what it points at: each finding ends with
  the `ppu docs` command for its note. One finding at a time: read its note,
  edit, check again. For an API, read its declaration in `ppu.d.luau` and
  write the call: if it is wrong, the check names the note. Never two reads
  in a row: write code or check between them. When the check passes, stop
  reading.
- `ppu docs <query>` prints one note: a diagnostic code a check printed
  (`ppu docs snes.budget`), an API name (`ppu docs sprites.group`), an error
  message or words. `ppu docs` lists the maps; `ppu docs <map>` lists a
  map's notes.
- `ppu.d.luau` in this folder declares every global the engine has. Look
  names up there; never guess them.
- `ppu docs cli` lists the notes on the commands, the manifest and `ppu connect`.

## The shape of a toy

`main.luau` holds `frame(t, f)`, called every video frame (`t` seconds, `f`
the frame index), and an optional `init()`. Other files are modules,
loaded with `require("./name")`. The SNES is flat globals: `mode`, `bg[n]`,
`m7`, `cgram[i]`, `hdma()`, `pad`, `voice[]`. Art is an image source (an
indexed PNG listed in `ppu.json`) placed with `dma("name")`.

Use the blessed APIs. They are written so the same code runs fast on a
cartridge, and toys that skipped them shipped broken:

- Sprites through `sprites.art` and `sprites.group`, drawn with
  `group:draw` every frame. Never write `obj[i]` by hand.
- Screens through `scenes`. Text and numbers through `require("@ppu/hud")`.
- `makeFixed(v, min, max, steps)` and `makeBounded(v, min, max)` for every
  piece of state that lasts between frames, written as you go, not after. A
  cartridge enforces a bound by stopping: make it the honest bound of every
  value the code can produce, and clamp or wrap the value itself.
- Big graphics (a Mode 7 plane, a full screen) as an image source placed with
  `dma()`; a level bigger than the screen as a Tiled map.
  Never `vram[]` loops in `init()`: a console shows black for seconds.
- `lookup` and `bakeHdma` to precompute math and scanline effects, `routine`
  for work spread over frames, `saveData` for battery saves.

## Exploit the PPU

Think in registers, tiles, palettes, sprites, compositing and changes along
the scanline, not a pixel canvas. Take every SNES effect the idea allows,
especially Mode 7 and HDMA, combined with windows, colour math, palette
animation, mosaic, sprite priorities and parallax. Prefer visible use of
these over a static image with a few particles. [recipes.md](recipes.md) has
worked recipes for the effects that make a toy look impossible; read the
ones you plan to use.

## Art

Draw at native scale with a
deliberate, limited palette per layer. `ppu art new` makes an empty PNG with
its source entry and palette, `ppu art convert` turns a full-colour PNG into
SNES art, and `ppu art check` says what an import would refuse: run it
first when art looks wrong. For generated art, keep a deterministic script in
the folder that writes the PNGs, so the art can be revised as code. `ppu`
imports PNGs; it does not generate images.

## The loop

Work in small steps and look at every one:

```sh
ppu check .  --duration 10                   # errors with file:line:frame, budgets, sprite overflow
ppu render . --at 0,2,4,6 -o previews        # native 256x224 PNGs; look at them
ppu render . --at 1,2,3 -o previews --hold r@30-120,a@90-92   # a game, with buttons held
```

Running without errors does not mean it looks right: look at the PNGs, name
what is weak (banding, illegible shapes, empty scenes, jitter, wrong motion),
fix it and render again. Judge games from motion across several frames with
input held, not from one idle still.
`scripts/contact.py` tiles a labelled contact sheet (needs Pillow);
`scripts/film.py` renders an MP4 (needs ffmpeg; silent).

A game keeps its state in Luau between frames; `pad` fields are true while
held, so keep the previous value to detect a press. A timed
piece derives motion from `t` so seeks and loops hold: `ppu check --seek` and
`--loop` prove it.

## Sound

`ppu docs audio` lists the sound notes; read `ppu docs score` before writing
music. Effects: `zap = bank("pluck")` at file level, then
`sfx(zap, "C5")`: the sound driver picks the voice, borrowing one from the
song until the effect ends (`ppu docs voices-for-songs-and-effects`); name
one, `sfx(zap, 6, "C5")`, only for effects that should cut each other off.
A looping sample stops after 0.15 s unless `bank()` or
`sfx(..., {len = 0.4})` sets `len`. `ppu check` runs the audio but cannot
play it: say you have not heard it.

Music is a `.song.json` listed in `ppu.json` as a `song` source (`ppu docs
song.json`). Run `ppu song check <file>` after every change and change one
thing because of a number; `ppu song roll <file> -o roll.png` to look at it;
`ppu render . --wav out.wav --duration 8` to measure the toy as it plays
(silent until it sets `dsp.mvol`). `ppu sfx <preset> -o sfx/name.wav` makes
an effect the bank lacks. `ppu docs compose` leads to the composing notes.

## Sharing and cartridges

In a folder `ppu connect` linked, every save syncs to the project and the
open Studio tab. Otherwise `ppu pack . -o toy.ppu.json` makes a file the
person opens in the Studio. Publishing is the person's decision.

A cartridge is built on ppu.toys: the Studio's SNES panel checks the toy
against the console as you edit, and Export to ROM builds the `.sfc`. Stay
inside the cartridge's subset from the start: `ppu check` runs the
cartridge check and ends each finding with the `ppu docs` command for its
note.

Signed in (`ppu login`), `ppu build` runs the same build from this folder,
prints the compiler's findings as `file:line: error[code]: message` and
writes `build/<folder>-<UTC time>.sfc` (`build/` is ignored and never synced).
`ppu check` passing does not mean the cartridge builds: run `ppu build` when
the person wants a cartridge or says it will not compile, and fix every
finding before building again. A build takes minutes (it waits up to 20 and
prints a line every 30 s): give the command a long timeout or run it in the
background, never kill and restart it. Builds are rate-limited, so build at
the end, not after every edit. A built image has compiled, not been played:
tell the person where the `.sfc` is and that it has not been run.
