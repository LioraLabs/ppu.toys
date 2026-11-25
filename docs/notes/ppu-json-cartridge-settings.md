---
id: ppu-json-cartridge-settings
kind: concept
title: targetHz and romSize in ppu.json are build settings, not source, and travel with the project
codes: []
---
`"targetHz"` is the project's cartridge setting for video region and update rate in one number: 60 or 30
for NTSC, 50 or 25 for PAL. Without it a project builds for NTSC. `"romSize"` is `"auto"` (the default, the
smallest size that holds the toy) or a size in KiB (256, 512, 1024, 2048 or 4096). Neither is read by the
code: the Studio's Cartridge settings edit them, `ppu pack`, `ppu unpack` and [[ppu-connect]] carry them,
and [[ppu-build]] and Export to ROM read them. See [[snes.target]] and [[snes.rom]] for what each costs and
refuses.

```sh
grep -E 'targetHz|romSize' ppu.json
```
