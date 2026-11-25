---
id: a-wav-file-is-a-sample-source
kind: recipe
title: A .wav file in the folder is a sample source, listed in ppu.json with kind sample
codes: []
---
A WAV in the folder becomes a sample when ppu.json lists it as a source of kind `sample`. Any WAV
works: integer or float samples of any width, any channel count, any rate. It is mixed to mono,
resampled to 32 kHz and BRR-encoded each time the toy loads, and refused past 3.64 seconds.
`options.loopStart` is the frame the loop returns to, counted at 32 kHz; leave it out for a one-shot.
Play it by name through [[bank]] with an `adsr` of its own ([[samples-are-sources]]).
[[ppu-sfx|ppu sfx]] writes such a WAV from a preset. When the Studio changes the sample in a way the
WAV cannot hold, the source moves to a payload under `.ppu/sources/` and `ppu connect` says the file is
no longer used; the WAV stays on disk. `meta.icon` picks the picture the Studio's Staff view draws for
it ([[a-sample-can-carry-an-icon]]).

```json
{"name": "zap", "kind": "sample", "file": "sfx/zap.wav", "options": {"loopStart": 1600}}
```
