---
id: ppu-render-wav
kind: recipe
title: ppu render --wav records what a toy plays to a WAV and prints its peak and RMS
codes: []
fixture: twinstar
---
`ppu render <dir> --wav out.wav --duration seconds` plays the toy from zero and writes the S-DSP's
output, stereo at 32 kHz, then prints its length and peak and RMS in dBFS. Hold buttons with `--hold`
as for pictures ([[ppu-render]]), so a game's sound effects fire. Unlike [[ppu-song-check|ppu song check]]
it includes the toy's `BANK` edits, echo and mixer, so it catches a song that clips once echo is on.

A toy is silent until it opens [[dsp.mvol]], which starts at 0 ([[sound-chip-powers-on-silent]]): a
render that prints `peak silent` usually means the master volume was never set. The agent cannot hear
the file; the numbers stand in, and the person listens.

```sh
mkdir -p build
ppu render . --wav build/play.wav --duration 6 --hold start@60-70,a@120-300
```
