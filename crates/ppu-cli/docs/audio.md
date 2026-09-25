# Audio

The SNES has a second machine for sound: the S-DSP, with its own 64 KB of
sound RAM (ARAM), eight voices, BRR-compressed samples, ADSR envelopes, and
an echo buffer. ppu.toys emulates that chip exactly and replaces its driver
CPU with your Lua, the same way it replaces the main CPU for graphics.

## Two clocks

`frame(t, f)` is the video clock — it runs once per rendered frame, 60 Hz.
`timer(n, div, fn)` is the audio clock: it registers one of the three real
SPC700 timers (the sound CPU's, which drives the S-DSP). `n = 0` or `n = 1`
tick at 8 kHz; `n = 2` ticks at 64 kHz. `div` (1–255) is how many ticks make
one fire. `timer()` is setup-only — call it at the top level or in `init()`,
like `dma()`.

Each frame renders 532 or 533 samples of audio at 32 kHz. A timer hook is
called as `fn(off)`, where `off` is the sample offset inside that frame's
span — the audio equivalent of `hdma()`'s scanline `y`.

Within one frame the order is: `frame()` runs first, and its `voice[]`,
`dsp`, `kon`, and `koff` writes land at sample offset 0; then timer hooks
fire in time order, each hook's writes landing at its own offset; then
`hdma()` hooks run. A few consequences follow directly from that order:

- A `kon()` or `voice[]` write made inside an `hdma()` hook lands at the
  **next** frame's offset 0, not the current one — `hdma()` runs after the
  audio pass has already been flushed for this frame.
- A timer hook may not write PPU registers, VRAM, or CGRAM. Only user
  globals cross from a hook back into `frame()`: set a value like `beat` in
  the hook, then read it and poke registers in `frame()`.
- Both hooks and `frame()` read the **previous** frame's `envx`, `outx`, and
  `ended` — audio read-backs lag one frame behind the writes.
- The first fire is one period after compile. Phase carries across frames
  once a timer is running; a fresh registration on recompile restarts it
  from zero.

```lua
beat = 0
timer(0, 250, function(off)
  beat = beat + 1
end)

function frame(t, f)
  brightness = beat % 16
end
```

## Voices

`voice[0]` through `voice[7]` are hardware-numbered, like `obj[]`. Each
voice is a table of fields:

- `sample` — a directory index (SRCN), the sample this voice plays.
- `pitch` — 14-bit; `0x1000` is 1:1, the sample's recorded rate.
- `vol = { l, r }` — signed −128..127 raw register values; negative inverts
  phase.
- `adsr = { a = 0..15, d = 0..7, s = 0..7, r = 0..31 }`.
- `gain` — set it and the voice switches to GAIN mode; `nil` keeps ADSR
  mode.
- `noise`, `pmod`, `echo` — booleans mapping to the NON, PMON, and EON bits.

Read-only per frame, and one frame behind as described above:

- `envx` (0..127) — the running envelope level.
- `outx` (−128..127) — the last output sample, post-envelope.
- `ended` — the raw ENDX bit; it also pulses when a looping sample passes
  its END block, not only when playback truly stops.

Voice fields persist across frames — the tables _are_ the register state.
Set what a voice needs once in `init()`, then change only what varies per
frame or per hit.

## Key on, key off

`kon(v, ...)` and `koff(v, ...)` take one or more voice numbers and set an
edge-triggered bitmask. Calling `kon` twice for the same voice in one frame
has the same effect as calling it once. Calling `koff` then `kon` for the
same voice within the same span restarts it — KON is flushed last.

```lua
local was_a = false
function frame(t, f)
  local pressed = pad.a and not was_a
  was_a = pad.a
  if pressed then kon(0) end
end
```

Check `voice[n].ended` to wait until a one-shot sample has finished before
reusing its voice.

## The mixer

`dsp` holds chip-wide state:

- `mvol = { l, r }` — master volume.
- `evol = { l, r }` — echo volume.
- `echo = { delay = 0..15, feedback, fir = { c0, c1, ..., c7 } }` — `fir` is
  a Lua array of eight signed FIR coefficients.
- `noise_clock` (0..31).
- `mute`.

Power-on `mvol` is `{ l = 0, r = 0 }` — nothing is audible until you set it,
except that starting a `song{}` or `score{}` while it is still zero opens it
to full, so a song is heard without a mixer line.

```lua
function init()
  dsp.mvol = { l = 127, r = 127 }
end
```

## Samples

In the Studio's Sources panel, drop a `.wav` (or any decodable audio file)
with kind Sample. It is mixed down to mono, resampled to 32 kHz, trimmed to
at most 3.64 seconds (7281 BRR blocks), optionally given a block-aligned
loop start, and encoded to BRR — 9 bytes per 16 samples — with a report
(blocks, bytes, SNR).

Place a sample with `dma()`, same call as backgrounds and sprites, but it
takes only an `addr` option (default `0x0500`) and returns `{ id, addr,
next_addr }`:

```lua
local kick = dma("kick")
voice[0].sample = kick.id
```

Without an `addr`, each placement lands right after the highest one so
far, so a sequence of `dma()` calls never overlaps; pass `addr` to pin one.
Here the same source twice, since this chapter has one sample; a second
`.wav` in the Sources panel gets its own name and `id`:

```lua
local kick = dma("kick")
local kick2 = dma("kick")
assert(kick2.addr == kick.next_addr)
```

Placement writes sound RAM **once**, at compile time — unlike VRAM, it is
not rebuilt every frame. `dma()` rejects a sample that exceeds sound RAM,
overlaps the sample directory page (`0x0100`–`0x04ff`), overlaps another
sample, or overlaps the echo region.

Raw pokes work anywhere: `aram[addr] = byte` lands at the next audio flush —
offset 0 for a write from `frame()`, or a hook's own offset from inside a
timer hook — and, like a placement, it is written once and never rebuilt.

## Echo RAM budget

The echo buffer lives at the **top** of sound RAM: `dsp.echo.delay * 2 KB`,
ending at `0xffff`. Delay 0 turns echo writes off, reserving only 4 bytes at
`0xff00`. `dma()` checks a sample's placement against `dsp.echo.delay` as it
stands after `init()` has run, wherever the `dma()` call sits — so setting
the delay in `init()` is enough; the check always sees the final budget.
The echo unit writes into sound RAM continuously while it is on, so a
sample placed inside the echo region would be overwritten on hardware —
the emulation refuses that placement for the same reason.

```lua
function init()
  dsp.echo = { delay = 4, feedback = 64, fir = { 127, 0, 0, 0, 0, 0, 0, 0 } }
  dsp.evol = { l = 48, r = 48 }
  voice[0].echo = true
end
```

A delay of 4 reserves the top 8 KB, leaving sound RAM below `0xe000` free
for samples. The inspector's Audio tab shows an ARAM budget bar: directory,
samples, echo, and free space.

## Music kit

Every toy and the CLI load a small Lua prelude before your code, defining
these globals (a user chunk that defines the same name wins):

- `note(n [, base])` — a note name (`"C4"`, `"C#4"`, `"Db4"`, `"A-1"`) or a
  MIDI number, converted to a 14-bit pitch relative to `base` (default
  `"C4"`, the note the sample was recorded at).
- `instrument{ sample =, adsr = ?, gain = ?, vol = 127, pan = 0, base =
"C4", pitch = 0x1000, noise = ?, pmod = ?, echo = ? }` — `pan` runs −1..1;
  `pitch` is what `sfx()` plays when given no note.
- `sfx(inst, v [, n])` — applies an instrument preset to `voice[v]` (`pan`
  folds into `vol.l`/`vol.r`) and keys it on. With no note it plays at the
  preset's `pitch` (default `0x1000`).
- `bank(name [, opts])` — places a [built-in sample](#built-in-samples) with
  `dma()` and returns its `instrument{}` preset; `opts` override any preset
  field (`vol`, `pan`, `adsr`, `addr`, ...). Setup-only.
- `song{ tempo, steps = 16, tracks = { { voice, inst, pattern = "C2 . - ^" }
} }` — runs on one `timer(0, ...)` and auto-plays. Pattern tokens: a note
  name keys the voice on, `^` keys it off, `.` and `-` do nothing. A pattern
  shorter than `steps` loops on its own length, so tracks can run
  polymeters against each other. Returns `{ step, beat, playing, div, rate,
play(), stop() }`. Like `timer()`, `song{}` is setup-only.
- `score{ song = "<id>" | data, loop = true }` — plays a
  [sequencer song](#sequencer-songs) on all eight voices, chosen per note.
  Setup-only. Returns `{ tick, length, playing, events, song, play(), stop() }`.

## Built-in samples

Every toy can `dma()` these names with nothing uploaded. They are
synthesized, chip-sized, and deterministic: the same bytes in the Studio,
the CLI, and a published toy. An uploaded source with the same name takes
the name over.

| Name                                             | Sound                                     | Bytes    |
| ------------------------------------------------ | ----------------------------------------- | -------- |
| `piano`                                          | bright attack settling into a mellow loop | 621      |
| `bass`                                           | warm, round low tone                      | 207      |
| `lead`                                           | 25% pulse, chip lead                      | 207      |
| `strings`                                        | sawtooth; give it a slow attack           | 207      |
| `organ`                                          | drawbar-style harmonics                   | 207      |
| `bell`                                           | inharmonic strike fading to a pure loop   | 621      |
| `flute`                                          | near-sine with a breathy attack           | 414      |
| `pluck`                                          | very bright attack, quick to mellow       | 1035     |
| `kick` `snare` `hat` `ohat` `tom` `clap` `crash` | a drum kit, one-shots                     | 630–4500 |

Melodic samples loop and are recorded at C4, so `note()` and `song{}`
pitch them correctly with the default `base`. Drums are recorded at 16 kHz
like period games: play them at pitch `0x0800`, which `bank()` does for you.

```lua
local piano = bank("piano")
local kick = bank("kick")
local hat = bank("hat", { vol = 70, pan = 0.4 })

local tune = song{ tempo = 110, tracks = {
  { voice = 0, inst = piano, pattern = "C4 . E4 . G4 . E4 ." },
  { voice = 1, inst = kick, pattern = "C4 . . . C4 . . ." },
  { voice = 2, inst = hat, pattern = ". . C4 . . . C4 ." },
} }
```

Raw `dma("kick")` works too and returns the usual `{ id, addr, next_addr }`;
built-ins and your own uploads share the same auto-chaining placement.

## Sequencer songs

A sequencer song is a function returning plain data: rows of sounds,
patterns of step strings, and an arrangement that chains them. `score{}`
plays it, either by name (`song = "beat1"` finds `seq_beat1`) or as a table
(`data = seq_beat1()`):

```lua
function seq_beat1() return {
  tempo = 120, swing = 0,
  rows = { { sound = "kick" }, { sound = "snare" }, { sound = "piano", note = "E4" }, { sound = "mybass" } },
  patterns = {
    A = { "4...4...4...4...", "....3.......3...", "2---....2---....", "3-..3-..3-..3-.." },
    B = { "4.4.4.4.4.4.4.4.", "....3.......3-3-", "................", "3-..3-..3-..3-.." },
  },
  arrangement = { "A", "A", "B", "A" },
} end

local beat = score{ song = "beat1" }
```

Each row is a sound at a pitch: a built-in name plays through `bank()` with
its preset envelope, anything else is an uploaded sample with a flat one.
`note` pitches the row with `note(row.note, base)`; a drum with no `note`
keeps its own pitch. Every pattern has one string per row, and the string's
length is its step count (8, 16 or 32 sixteenths). In a string, `1` to `4`
is a hit at volume 32, 64, 96 or 127 (scaled by the sound's own volume),
`-` holds the hit before it, and `.` rests. `tempo` runs 1 to 400 BPM;
`swing` (0 to 75) delays every odd step by that percentage of a step.

`score{}` compiles the whole arrangement once, at setup, into
`beat.events`: one `{ start, ["end"], voice, row, pitch, l, r }` per note,
in 4 ms ticks, ordered by start. Voices are picked note by note: the lowest
voice whose note has ended, or, when all eight are sounding, the note that
started earliest is cut — or, among notes that started together, the one on
the lowest voice. So a chord of nine held notes doesn't go silent, it steals
a voice from an earlier note instead. A bad step string, a pattern the
arrangement names but doesn't define, or a sound that is neither built in
nor uploaded stops the program with an error naming the row or pattern.

The player runs on one `timer(0, 32, ...)`: it keys off notes that have
ended, then starts the notes due. At the end it keys every voice off and,
unless `loop = false`, starts again from tick 0. `beat.tick` and
`beat.length` are the position and length in ticks; `beat.stop()` keys the
voices off and pauses, `beat.play()` resumes.

A song played by name reloads in place. When a file defines `seq_<id>` and
nothing else at the top level, editing it while the song plays doesn't
restart the toy. The engine re-runs the file, and every `score{ song = "<id>" }`
recompiles from the new data and keeps its place in the song: the same step,
the same distance into it, capped at the step's end if the step got shorter.
A tempo or swing edit moves the step's tick, so step 8 stays step 8 when the
tempo halves. Notes sounding at the moment of the edit are keyed off, the
next note due plays on time, and a song that got shorter than its position
wraps (or stops, with `loop = false`). If the new data has an error, the old
song keeps playing and the error names the file. Editing a song file that no
score plays doesn't restart the toy either: the file is re-run and anything
playing carries on.

There are three cases where the edit restarts the toy the usual way. One
is a row naming a sound the song didn't have at setup, because sounds are
placed only at setup. Another is a file that runs any other top-level code.
The third is a song file no `score{ song = ... }` plays, while a
`score{ data = ... }` is set up: that table may have come from the edited
song, so the edit restarts the toy.

## Music from a MIDI file

Drop a `.mid` on the Sources panel and it becomes a
[sequencer song](#sequencer-songs) file named after it: notes snap to the
nearest sixteenth, each distinct track sound and key becomes a row (up to
24, the most used kept), and every two bars become a pattern, with repeats
shared through the arrangement. The import is one-way; the song file is
ordinary Lua from then on, played by `score{}` like any other.

## Your first note

This toy plays a bass line automatically, fires a hit when you press A, and
drives brightness from the bass voice's envelope. `kick` is a built-in
sample, so it runs as-is; upload your own `kick` to replace it.

```lua
-- ppu.toys tutorial :: first-note — a sample, a song, an sfx on A, and a light that follows the envelope
-- The sound chip (S-DSP) is a second machine bolted onto the console: its
-- own 64 KB of sound RAM (ARAM), its own clock, its own eight voices. dma()
-- on a sample source copies BRR-encoded bytes into that RAM and hands back
-- a directory entry (kick.id is the SRCN a voice points at). song{} steps a
-- pattern on a hardware timer, same idea as hdma but for sound. sfx() keys
-- a voice on demand; voice[n].envx is that voice's live envelope — the same
-- number you can wire straight into the picture.
local kick = dma("kick")                          -- BRR bytes land in sound RAM at 0x0500 + a directory entry; kick.id is the SRCN
local bass = instrument{ sample = kick.id, adsr = { a = 15, d = 4, s = 4, r = 8 } }
local hit  = instrument{ sample = kick.id, adsr = { a = 15, d = 0, s = 7, r = 0 }, pan = 0.5, base = "C3" }
local tune = song{ tempo = 120, tracks = {
  { voice = 0, inst = bass, pattern = "C2 . . G2 . . C3 ." },
} }

function init()
  dsp.mvol = { l = 127, r = 127 }                 -- power-on MVOL is 0 (silent) — turn the speakers on
end

local was_a = false
function frame(t, f)
  local pressed = pad.a and not was_a             -- edge, not level: sfx() restarts the voice every call
  was_a = pad.a
  if pressed then sfx(hit, 1, "C4") end
  brightness = floor(voice[0].envx / 8)           -- ENVX is 0..127; brightness is 0..15
  cgram[0] = hsl(200 + tune.step * 12, 0.6, 0.3)  -- the backdrop hue drifts with the song's own step counter
end
-- Try: swap "C4" for a different sfx pitch, hold A twice fast to hear the retrigger, or drive brightness from voice[1].envx instead
```

## Audio-reactive visuals

`voice[n].envx` and `voice[n].outx` are a ready-made input for brightness,
color, or scroll. The toy above already does this with `voice[0].envx`.

## Run vs recompile

Editing a toy recompiles it, and a recompile hot-reloads: globals are
rebuilt, but the chip keeps running — a sounding voice keeps sounding,
samples stay where they were placed in sound RAM, and timers re-register at
phase zero.

Pressing Run (`t = 0`) power-cycles instead: a fresh chip, sound RAM
zeroed, samples re-placed, timers restarted. Run always produces the same
bytes.

The speaker button on the output — and on wall cards and players — mutes
playback locally only; a recording captures audio even while the local
output is muted. Chrome requires one click on the output before it will
play any audio at all.

## Register table

| Register      | Lua                                                 | Meaning                                                               |
| ------------- | --------------------------------------------------- | --------------------------------------------------------------------- |
| VOL (L/R)     | `voice[n].vol.l` / `.r`                             | Per-voice volume                                                      |
| P (PITCH L/H) | `voice[n].pitch`                                    | 14-bit playback rate                                                  |
| SRCN          | `voice[n].sample`                                   | Sample directory index                                                |
| ADSR1/ADSR2   | `voice[n].adsr`                                     | Envelope rates                                                        |
| GAIN          | `voice[n].gain`                                     | Direct envelope control (non-nil switches out of ADSR)                |
| ENVX/OUTX     | `voice[n].envx` / `.outx`                           | Read-only, previous frame                                             |
| KON/KOFF      | `kon()` / `koff()`                                  | Edge-triggered key on/off                                             |
| ENDX          | `voice[n].ended`                                    | Raw end-of-sample bit                                                 |
| MVOL          | `dsp.mvol`                                          | Master volume                                                         |
| EVOL          | `dsp.evol`                                          | Echo volume                                                           |
| EFB           | `dsp.echo.feedback`                                 | Echo feedback                                                         |
| FLG           | `dsp.mute` · `dsp.noise_clock` · echo-write-disable | Mute, noise clock, and the echo-write-off bit (set when `delay` is 0) |
| PMON          | `voice[n].pmod`                                     | Pitch modulation enable                                               |
| NON           | `voice[n].noise`                                    | Noise enable                                                          |
| EON           | `voice[n].echo`                                     | Echo enable                                                           |
| DIR           | fixed `0x0100`                                      | Sample directory base; `dma()` fills it                               |
| ESA/EDL       | derived from `dsp.echo.delay`                       | Echo buffer start and length                                          |
| C0–C7         | `dsp.echo.fir`                                      | FIR filter coefficients                                               |

See also: [Sources](sources.md), [`dma()`](dma.md), [Controller input](pad.md),
[Scanline effects](scanlines.md).

## Live mixer

The Studio Audio panel edits sound while the song keeps running. Each of the eight
voices has volume trims, semitone transposition, echo, mute, and solo. Trims and
transposition follow the song's changing notes; mute and solo leave the envelopes
and sequencer running.

Select a voice for envelope, sample, and advanced controls. Pin a field to keep
its value across song writes; release it to follow the song again. Sample changes
take effect on the next note. Trigger and Release note send one key event while
playing. Raw signed volume values retain phase inversion. Master/echo trims,
delay, feedback, noise clock, and FIR taps are below the voice controls. Echo delay
changes that overlap loaded samples are rejected.

Adjustments are live until **Save mix** writes `audio.mix.json` into the toy's
files. This file travels with exports and published toys; the core reads it as
mixer settings, never as Lua. Saving only the mix does not recompile the song or
restart its timers. **Load saved** discards live changes; **Release all** clears
all adjustments (save afterward to persist that reset). Opening another toy
clears the previous toy's live adjustments.
