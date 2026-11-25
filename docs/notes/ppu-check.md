---
id: ppu-check
kind: recipe
title: ppu check plays a toy for its length and fails on the first error, sprite overflow or broken bound
codes: []
---
`ppu check <dir>` runs the toy in the preview engine for 30 seconds (`--duration` to change it), every
frame, and exits non-zero on a runtime error, a sprite line over its limit (`--allow-overflow` when the
overflow is meant), or a value stored outside its [[makeBounded]] or [[makeFixed]] bounds: the preview
runs on, a cartridge stops at the first. Hold buttons with `--hold` to drive a game through its states.

Beside the preview it runs the cartridge check over the same frames: what a cartridge build would refuse,
warn about or stop on. Each finding ends with the `ppu docs` command that opens its note. The check comes
from the site, downloaded once per version, checked against the hash the site serves and cached; offline,
`ppu check` uses the cached one and says so. Offline with nothing cached, it prints `NOT CHECKED` beside
the preview's `PASS`: only the preview ran.

Run it after each change; when it passes, stop reading docs and move on.

```sh
ppu check .
ppu check . --duration 60 --hold a@60-70,r@120-300
```
