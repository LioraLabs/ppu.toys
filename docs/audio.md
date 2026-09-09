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

Voice fields persist across frames — the tables *are* the register state.
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

Power-on `mvol` is `{ l = 0, r = 0 }` — nothing is audible until you set it.

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

Chain a second placement below the first with its `next_addr` — here the
same source twice, since this chapter has one sample; a second `.wav` in
the Sources panel gets its own name and `id`:

```lua
local kick = dma("kick")
local kick2 = dma("kick", { addr = kick.next_addr })
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
  "C4", noise = ?, pmod = ?, echo = ? }` — `pan` runs −1..1.
- `sfx(inst, v [, n])` — applies an instrument preset to `voice[v]` (`pan`
  folds into `vol.l`/`vol.r`) and keys it on.
- `song{ tempo, steps = 16, tracks = { { voice, inst, pattern = "C2 . - ^" }
  } }` — runs on one `timer(0, ...)` and auto-plays. Pattern tokens: a note
  name keys the voice on, `^` keys it off, `.` and `-` do nothing. A pattern
  shorter than `steps` loops on its own length, so tracks can run
  polymeters against each other. Returns `{ step, beat, playing, div, rate,
  play(), stop() }`. Like `timer()`, `song{}` is setup-only.

## Your first note

This toy plays a bass line automatically, fires a hit when you press A, and
drives brightness from the bass voice's envelope. It needs a sample source
named `kick` in the Sources panel.

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
  apply_pokes()
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

| Register | Lua | Meaning |
|---|---|---|
| VOL (L/R) | `voice[n].vol.l` / `.r` | Per-voice volume |
| P (PITCH L/H) | `voice[n].pitch` | 14-bit playback rate |
| SRCN | `voice[n].sample` | Sample directory index |
| ADSR1/ADSR2 | `voice[n].adsr` | Envelope rates |
| GAIN | `voice[n].gain` | Direct envelope control (non-nil switches out of ADSR) |
| ENVX/OUTX | `voice[n].envx` / `.outx` | Read-only, previous frame |
| KON/KOFF | `kon()` / `koff()` | Edge-triggered key on/off |
| ENDX | `voice[n].ended` | Raw end-of-sample bit |
| MVOL | `dsp.mvol` | Master volume |
| EVOL | `dsp.evol` | Echo volume |
| EFB | `dsp.echo.feedback` | Echo feedback |
| FLG | `dsp.mute` · `dsp.noise_clock` · echo-write-disable | Mute, noise clock, and the echo-write-off bit (set when `delay` is 0) |
| PMON | `voice[n].pmod` | Pitch modulation enable |
| NON | `voice[n].noise` | Noise enable |
| EON | `voice[n].echo` | Echo enable |
| DIR | fixed `0x0100` | Sample directory base; `dma()` fills it |
| ESA/EDL | derived from `dsp.echo.delay` | Echo buffer start and length |
| C0–C7 | `dsp.echo.fir` | FIR filter coefficients |

See also: [Sources](sources.md), [`dma()`](dma.md), [Controller input](pad.md),
[Scanline effects](scanlines.md).
