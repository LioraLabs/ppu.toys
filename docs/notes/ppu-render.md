---
id: ppu-render
kind: recipe
title: ppu render writes the frames a toy shows at given times as PNGs, to judge how it looks
codes: []
---
Running without errors does not mean a toy looks right. `ppu render <dir> --at seconds,... -o <directory>`
plays the toy from zero to the last time asked for and writes one PNG per time (`frame-00000060.png` for
1 s), the same pixels the Studio shows. Times round to the nearest 60 Hz frame.

For a game, hold buttons with `--hold`: `r@450-460` holds right on frames 450 to 459, `2.a@600-610` the
second pad's A. Judge motion from several frames, not one still, and keep the last time small: [[render-a-few-moments|render a few moments]]. After the pictures look right, run
[[ppu-check|ppu check]] over the toy's length.

```sh
ppu render . --at 0.5,1,2 -o build/shots
ppu render . --at 8,8.25,8.5 --hold r@450-520 -o build/shots
```
