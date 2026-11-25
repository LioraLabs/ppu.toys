---
id: connected-folder
kind: concept
title: A connected folder syncs only what ppu.json names, so generated output belongs in build/
codes: []
---
[[ppu-connect|ppu connect]] syncs the code files in ppu.json's `files` and the sources in its
`sources` (each PNG or `.gpl` a source's `file` points at). Everything else in the folder stays
local: a new `.luau` file reaches the Studio only once it is listed in `files`, and art only once
a source names it.

Put what you generate (rendered frames, built cartridges, scratch scripts) under `build/`.
[[ppu-build|ppu build]] writes its `.sfc` there and leaves a `.gitignore` in it, so neither git nor
the project ever sees it. Keep it out of `assets/`, where a stray PNG is easy to mistake for art.

```sh
ppu render . --at 1 -o build/shots
ppu check .
```
