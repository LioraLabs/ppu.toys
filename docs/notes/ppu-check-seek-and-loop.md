---
id: ppu-check-seek-and-loop
kind: recipe
title: ppu check --seek and --loop compare a toy's pixels after jumps and across a loop, to prove it is seekable
codes: []
---
A timeline toy (a performance of fixed length) must look the same whether it plays straight through or
is jumped to, because the Studio's scrubber seeks. `--seek 0,15.9833,16,48` compares sample frames from
continuous playback with a fresh seek and a backward seek to each of those times; `--loop 96` adds samples
at zero, each quarter and the last frame before the loop, and compares them one period later. Times round
to the nearest 60 Hz frame and each must fall within `--duration`. `check` never infers a period from the
code: keep `--loop` equal to the toy's own period and `--duration` to its length. A game that keeps state
need not pass these; sprite overflow is checked on every frame either way.

```sh
ppu check . --duration 4 --seek 0,1.0167,2,3 --loop 4
```
