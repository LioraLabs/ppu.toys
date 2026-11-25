---
id: samples-are-sources
kind: recipe
title: An uploaded sample is a source, played by name through bank() with an envelope of its own
codes: []
---
A sample source is a WAV (any rate, mono or stereo): a `.wav` in the folder listed in ppu.json with
kind `sample` ([[a-wav-file-is-a-sample-source]]), one dropped in the Studio's Sources panel with kind
Sample, or one sent with `add_sample_source` (base64 WAV, optional `loop_start` in samples at 32 kHz).
[[ppu-sfx|ppu sfx]] makes one from a preset. It becomes mono 32 kHz BRR, at most 3.64 seconds, in the 64 KiB of sound RAM the samples, songs and echo
buffer share ([[snes.audio_budget]]: a sample declared in a scene's own file uploads with that scene only): keep it short, a one-shot drum or a short attack with a loop recorded at C4.

[[bank]]`("name", opts)` places a built-in or uploaded sample at file level and returns its
instrument; an uploaded one has no preset, so give it `adsr`. `dma("name")` on a sample takes only
`addr` and runs only at setup.

```luau
-- A built-in here; an uploaded sample is named the same way, with {adsr = {a = 15, d = 4, s = 6, r = 8}}.
local zap = bank("pluck")

function frame(t, f)
  if pad.a then
    sfx(zap, 5, "C5")
  end
end
```
