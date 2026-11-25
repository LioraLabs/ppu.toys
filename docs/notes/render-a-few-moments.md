---
id: render-a-few-moments
kind: recipe
title: To see a game move, render a few moments close together with --hold, not frames spread over minutes
codes: []
---
[[ppu-render|ppu render]] plays the toy from zero up to the last time asked for, so the cost is
that last time, not how many PNGs you ask for. `--at` is in seconds: `--at 5,100,200,400,900` plays
fifteen minutes of game and runs for minutes. Nothing in a game happens at 900 s that input cannot
reach in ten.

Pick the moment, reach it with `--hold` (frames, 60 a second), and render three or four times a
quarter second apart around it: one still cannot show motion, and close frames show which way it
goes. Write them into `build/` (see [[connected-folder]]).

```sh
ppu render . --at 2 -o build/shots
ppu render . --at 4,4.25,4.5 --hold a@60-70,r@120-260 -o build/shots
```
