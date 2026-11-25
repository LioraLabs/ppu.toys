---
id: ppu-build
kind: recipe
title: ppu build makes the cartridge once the preview is right, and keeps each one in build/
codes: []
---
`ppu build [dir]` sends the folder to the SNES compiler on the site you are signed in to (the
Studio's Export to ROM), prints each refusal as `file:line` with its code (`ppu docs <code>` tells
you the fix), and writes `build/<folder>-<UTC time>.sfc`; `-o out.sfc` names the file instead. It
needs `ppu login` first, and takes minutes, so build once, at the end.

Get the toy right in the preview first: [[render-a-few-moments|render a few moments]] to see it,
then [[ppu-check|ppu check]] over its length, which catches a runtime error or broken bound before
the compiler does. Then build, sign-in check first:

```sh
ppu check .
ppu whoami || echo "not signed in: run ppu login, then ppu build"
```
