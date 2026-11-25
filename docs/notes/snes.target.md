---
id: snes.target
kind: diagnostic
title: Region and update rate are build settings in ppu.json, not a target() call in code
codes: [snes.target]
---
`error[snes.target]` means the code calls `target()`. The video region and the update rate belong to
the project, not the source: set `"targetHz"` in ppu.json (60 or 30 for NTSC, 50 or 25 for PAL), or
choose them in the Studio's Cartridge settings. Delete the call.

A game runs at 60 Hz by default and slows down as the console did when an update overruns; 30 Hz is
a choice the author makes in that setting, not something the code asks for.

```luau
-- ppu.json: "targetHz": 30
function frame(t, f)
  brightness = 15
end
```
